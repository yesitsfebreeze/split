use anyhow::{anyhow, Context, Result};
use std::path::{Path, PathBuf};

use crate::plugin;

pub fn stitch(skeleton_path: &Path) -> Result<String> {
    let skeleton = std::fs::read_to_string(skeleton_path)
        .with_context(|| format!("read {}", skeleton_path.display()))?;

    let comment = comment_for_skel(skeleton_path);
    let source_prefix = format!("{} §source ", comment);
    let ref_prefix = format!("{} §", comment);

    let mut out = String::with_capacity(skeleton.len() * 2);

    for line in skeleton.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with(&source_prefix) {
            continue;
        } else if let Some(ref_path) = trimmed.strip_prefix(&ref_prefix) {
            let body = load_body(Path::new(ref_path), skeleton_path, &comment)?;
            out.push_str(&body);
            out.push('\n');
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }

    Ok(out)
}

pub fn source_path_from_skel(skel_path: &Path) -> Result<PathBuf> {
    let content = std::fs::read_to_string(skel_path)
        .with_context(|| format!("read {}", skel_path.display()))?;
    let comment = comment_for_skel(skel_path);
    let source_prefix = format!("{} §source ", comment);
    for line in content.lines() {
        if let Some(src) = line.strip_prefix(&source_prefix) {
            return Ok(PathBuf::from(src.trim()));
        }
    }
    Err(anyhow!("no §source header in {}", skel_path.display()))
}

fn load_body(body_path: &Path, skeleton_path: &Path, comment: &str) -> Result<String> {
    let raw = if body_path.is_absolute() && body_path.exists() {
        std::fs::read_to_string(body_path)?
    } else if body_path.exists() {
        std::fs::read_to_string(body_path)?
    } else if let Some(parent) = skeleton_path.parent() {
        let rel = parent.join(body_path);
        if rel.exists() {
            std::fs::read_to_string(&rel)?
        } else {
            return Err(anyhow!("body file not found: {}", body_path.display()));
        }
    } else {
        return Err(anyhow!("body file not found: {}", body_path.display()));
    };

    Ok(strip_markers(&raw, comment))
}

fn strip_markers(content: &str, comment: &str) -> String {
    let head = format!("{} §head", comment);
    let foot = format!("{} §foot", comment);
    let lines: Vec<&str> = content.lines().collect();
    let start = if lines.first().map_or(false, |l| l.starts_with(&head)) { 1 } else { 0 };
    let end = if lines.last().map_or(false, |l| l.starts_with(&foot)) {
        lines.len().saturating_sub(1)
    } else {
        lines.len()
    };
    lines[start..end].join("\n")
}

pub fn skeleton_path(src: &Path, index_dir: &Path) -> PathBuf {
    let ext = src.extension().and_then(|e| e.to_str()).unwrap_or("rs");
    index_dir.join(src.with_extension(format!("skel.{ext}")))
}

pub fn comment_for_skel(skel_path: &Path) -> String {
    let ext = skel_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("rs");
    plugin::meta_for_ext(ext).comment
}

const MARKER: char = '§';

pub fn marker_payload(line: &str) -> Option<&str> {
    let t = line.trim_start();
    let idx = t.find(MARKER)?;
    let prefix = &t[..idx];
    if prefix.len() > 4 {
        return None;
    }
    if !prefix.bytes().all(|b| b == b' ' || (!b.is_ascii_alphanumeric() && b != b'_')) {
        return None;
    }
    Some(&t[idx + MARKER.len_utf8()..])
}

pub fn is_marker_line(line: &str) -> bool {
    marker_payload(line).is_some()
}
