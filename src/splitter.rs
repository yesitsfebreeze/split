use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub struct BodyFile {
    pub path: PathBuf,
    pub content: String,
}

pub fn wrap_body(
    comment: &str,
    src_display: &str,
    name: &str,
    raw: &str,
    line_start: usize,
    line_end: usize,
) -> String {
    format!(
        "{c} §head {src}:{ls}-{le} {n}\n{raw}\n{c} §foot {src} {n}",
        c = comment, src = src_display, ls = line_start, le = line_end, n = name, raw = raw
    )
}

pub fn split_for_ext(source_path: &Path, index_dir: &Path, ext: &str) -> Result<(String, Vec<BodyFile>)> {
    if let Some(wasm) = crate::language::load(ext) {
        if let Ok(result) = crate::language::split(&wasm, ext, source_path, index_dir) {
            return Ok(result);
        }
    }
    if ext == "rs" {
        split(source_path, index_dir)
    } else {
        split_generic(source_path, index_dir)
    }
}

pub fn split_generic(source_path: &Path, index_dir: &Path) -> Result<(String, Vec<BodyFile>)> {
    let source = std::fs::read_to_string(source_path)
        .with_context(|| format!("read {}", source_path.display()))?;
    let ext = source_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let comment = crate::language::meta_for_ext(ext).comment;
    let src_display = to_slash(source_path);
    let body_dir = index_dir.join(source_path.with_extension(""));
    let body_path = body_dir.join("_body.fs");
    let body_path_slash = to_slash(&body_path);
    let total_lines = source.lines().count().max(1);
    let body_content = wrap_body(&comment, &src_display, "_body", source.trim_end(), 1, total_lines);
    let skeleton = format!("{c} §source {src_display}\n{c} §{body_path_slash}\n", c = comment);
    Ok((skeleton, vec![BodyFile { path: body_path, content: body_content }]))
}

pub fn split(source_path: &Path, impl_dir: &Path) -> Result<(String, Vec<BodyFile>)> {
    let source = std::fs::read_to_string(source_path)
        .with_context(|| format!("read {}", source_path.display()))?;

    let src_display = to_slash(source_path);
    let funcs = find_fns(&source);
    let comment = "//";

    let header = format!("// §source {src_display}\n");
    let header_len = header.len() as i64;
    let mut skeleton = header + &source;
    let mut bodies = Vec::new();
    let mut offset: i64 = header_len;

    for f in funcs {
        let raw_body = strip_body_edges(&source[f.body_start..f.body_end]);
        let body_dir = impl_dir.join(source_path.with_extension(""));
        let body_path = body_dir.join(format!("{}.fs", f.name));
        let body_path_slash = to_slash(&body_path);

        let line_start = line_of(&source, f.decl_start);
        let line_end = line_of(&source, f.body_close);
        let body_content = wrap_body(comment, &src_display, &f.name, &raw_body, line_start, line_end);

        let ref_text = format!("\n    // §{}\n", body_path_slash);
        let a = (f.body_start as i64 + offset) as usize;
        let b = (f.body_end as i64 + offset) as usize;
        skeleton.replace_range(a..b, &ref_text);
        offset += ref_text.len() as i64 - (f.body_end - f.body_start) as i64;

        bodies.push(BodyFile { path: body_path, content: body_content });
    }

    Ok((skeleton, bodies))
}

fn line_of(source: &str, byte_offset: usize) -> usize {
    let end = byte_offset.min(source.len());
    source.as_bytes()[..end].iter().filter(|&&b| b == b'\n').count() + 1
}

struct FnLoc {
    name: String,
    decl_start: usize,
    body_start: usize,
    body_end: usize,
    body_close: usize,
}

fn find_fns(source: &str) -> Vec<FnLoc> {
    let bytes = source.as_bytes();
    let mut result = Vec::new();
    let mut i = 0;

    while i < bytes.len() {
        // Skip line comments
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'/' {
            while i < bytes.len() && bytes[i] != b'\n' { i += 1; }
            continue;
        }
        // Skip block comments
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') { i += 1; }
            i += 2;
            continue;
        }
        // Skip string literals
        if bytes[i] == b'"' {
            i = skip_string(bytes, i + 1);
            continue;
        }
        // Skip raw string literals r#"..."# or r"..."
        if bytes[i] == b'r'
            && i + 1 < bytes.len()
            && (bytes[i + 1] == b'#' || bytes[i + 1] == b'"')
        {
            if let Some(j) = skip_raw_string(bytes, i) {
                i = j;
                continue;
            }
        }

        // Check for `fn` keyword
        if i + 2 <= bytes.len() && &bytes[i..i + 2] == b"fn" {
            let pre_ok = i == 0 || !is_ident_char(bytes[i - 1]);
            let post_ok = i + 2 >= bytes.len() || !is_ident_char(bytes[i + 2]);

            if pre_ok && post_ok {
                let name_start = skip_ws(bytes, i + 2);
                if name_start < bytes.len() && is_ident_start(bytes[name_start]) {
                    let name_end = ident_end(bytes, name_start);
                    let name = String::from_utf8_lossy(&bytes[name_start..name_end]).to_string();

                    if let Some(open) = find_open_brace(bytes, name_end) {
                        if let Some(close) = find_close_brace(bytes, open) {
                            result.push(FnLoc {
                                name,
                                decl_start: i,
                                body_start: open + 1,
                                body_end: close,
                                body_close: close,
                            });
                            i = close + 1;
                            continue;
                        }
                    }
                }
            }
        }

        i += 1;
    }

    result
}

fn find_open_brace(bytes: &[u8], from: usize) -> Option<usize> {
    let mut i = from;
    let mut paren = 0i32;
    let mut angle = 0i32;

    while i < bytes.len() {
        match bytes[i] {
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'/' => {
                while i < bytes.len() && bytes[i] != b'\n' { i += 1; }
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => {
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') { i += 1; }
                i += 2;
                continue;
            }
            b'(' => paren += 1,
            b')' => paren -= 1,
            b'<' if paren == 0 => angle += 1,
            b'>' if paren == 0 && angle > 0 => angle -= 1,
            b';' if paren == 0 && angle == 0 => return None, // trait fn declaration
            b'{' if paren == 0 && angle == 0 => return Some(i),
            _ => {}
        }
        i += 1;
    }
    None
}

fn find_close_brace(bytes: &[u8], open: usize) -> Option<usize> {
    let mut depth = 1i32;
    let mut i = open + 1;

    while i < bytes.len() {
        match bytes[i] {
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'/' => {
                while i < bytes.len() && bytes[i] != b'\n' { i += 1; }
                continue;
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => {
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') { i += 1; }
                i += 2;
                continue;
            }
            b'"' => { i = skip_string(bytes, i + 1); continue; }
            b'r' if i + 1 < bytes.len() && (bytes[i + 1] == b'#' || bytes[i + 1] == b'"') => {
                if let Some(j) = skip_raw_string(bytes, i) { i = j; continue; }
            }
            b'\'' if i + 2 < bytes.len() => {
                // Char literal (not lifetime: lifetime is 'a followed by ident chars without closing ')
                let next = bytes[i + 1];
                if next == b'\\' {
                    // escape sequence
                    i += 3; // skip '\X'
                    if i < bytes.len() && bytes[i] == b'\'' { i += 1; }
                    continue;
                } else if i + 2 < bytes.len() && bytes[i + 2] == b'\'' {
                    i += 3; // skip 'X'
                    continue;
                }
            }
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 { return Some(i); }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn skip_string(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() {
        if bytes[i] == b'\\' { i += 2; continue; }
        if bytes[i] == b'"' { return i + 1; }
        i += 1;
    }
    i
}

fn skip_raw_string(bytes: &[u8], start: usize) -> Option<usize> {
    let mut i = start + 1; // skip 'r'
    let h0 = i;
    while i < bytes.len() && bytes[i] == b'#' { i += 1; }
    let hashes = i - h0;
    if i >= bytes.len() || bytes[i] != b'"' { return None; }
    i += 1;
    loop {
        if i >= bytes.len() { return Some(i); }
        if bytes[i] == b'"' {
            let mut j = i + 1;
            let mut count = 0;
            while j < bytes.len() && bytes[j] == b'#' { count += 1; j += 1; }
            if count >= hashes { return Some(j); }
        }
        i += 1;
    }
}

fn skip_ws(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && matches!(bytes[i], b' ' | b'\t' | b'\n' | b'\r') { i += 1; }
    i
}

fn is_ident_char(b: u8) -> bool { b.is_ascii_alphanumeric() || b == b'_' }
fn is_ident_start(b: u8) -> bool { b.is_ascii_alphabetic() || b == b'_' }

fn ident_end(bytes: &[u8], start: usize) -> usize {
    let mut i = start;
    while i < bytes.len() && is_ident_char(bytes[i]) { i += 1; }
    i
}

fn strip_body_edges(s: &str) -> String {
    let s = s.strip_prefix("\r\n").or_else(|| s.strip_prefix('\n')).unwrap_or(s);
    s.trim_end().to_string()
}

pub fn to_slash(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

pub fn skeleton_path(src: &Path, index_dir: &Path) -> PathBuf {
    let ext = src.extension().and_then(|e| e.to_str()).unwrap_or("rs");
    index_dir.join(src.with_extension(format!("skel.{ext}")))
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
