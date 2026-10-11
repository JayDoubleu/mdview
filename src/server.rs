//! Handler for the `mdview://` custom protocol: static assets plus a tiny JSON API
//! that the frontend uses to list directories, render files and load local images.

use crate::render;
use percent_encoding::percent_decode_str;
use serde_json::{Value, json};
use std::borrow::Cow;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use wry::http::{Request, Response, header::CONTENT_TYPE};

include!(concat!(env!("OUT_DIR"), "/assets.rs"));

pub const MARKDOWN_EXTS: &[&str] = &["md", "markdown", "mdown", "mkd", "mkdn", "mdx", "mdwn"];

pub fn is_markdown(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| MARKDOWN_EXTS.contains(&e.to_ascii_lowercase().as_str()))
}

/// What the app was launched with.
pub struct Launch {
    pub root: PathBuf,
    pub file: Option<PathBuf>,
    pub error: Option<String>,
}

type Resp = Response<Cow<'static, [u8]>>;

pub fn handle(launch: &Launch, req: Request<Vec<u8>>) -> Resp {
    let uri = req.uri();
    let path = uri.path();
    let query = parse_query(uri.query().unwrap_or(""));
    let q = |k: &str| query.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());

    let result = match path {
        "/" | "/index.html" => asset("index.html"),
        "/hl.css" => {
            static CSS: OnceLock<String> = OnceLock::new();
            let css = CSS.get_or_init(render::highlight_css);
            Ok(respond("text/css", Cow::Borrowed(css.as_bytes())))
        }
        "/api/init" => Ok(json_resp(json!({
            "root": ps(&launch.root),
            "file": launch.file.as_deref().map(ps),
            "error": launch.error,
            "sep": std::path::MAIN_SEPARATOR.to_string(),
            "version": env!("CARGO_PKG_VERSION"),
        }))),
        "/api/list" => list(q("path").unwrap_or("."), q("all") == Some("1")),
        "/api/render" => render_file(q("path").unwrap_or("")),
        "/api/source" => source_file(q("path").unwrap_or("")),
        "/api/resolve" => Ok(json_resp(resolve(
            q("base").unwrap_or(""),
            q("rel").unwrap_or(""),
        ))),
        "/fs" => local_file(q("base").unwrap_or(""), q("rel").unwrap_or("")),
        p => asset(p.trim_start_matches('/')),
    };
    result.unwrap_or_else(|(status, msg)| {
        Response::builder()
            .status(status)
            .header(CONTENT_TYPE, "text/plain; charset=utf-8")
            .body(Cow::Owned(msg.into_bytes()))
            .unwrap()
    })
}

type Res = Result<Resp, (u16, String)>;

fn respond(mime: &str, body: Cow<'static, [u8]>) -> Resp {
    Response::builder()
        .header(CONTENT_TYPE, mime)
        .header("Cache-Control", "no-store")
        .header("X-Content-Type-Options", "nosniff")
        .body(body)
        .unwrap()
}

fn json_resp(v: Value) -> Resp {
    respond("application/json", Cow::Owned(v.to_string().into_bytes()))
}

fn asset(name: &str) -> Res {
    ASSETS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(n, data)| respond(mime_for(n), Cow::Borrowed(*data)))
        .ok_or((404, format!("not found: {name}")))
}

fn mime_for(name: &str) -> &'static str {
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "pdf" => "application/pdf",
        "txt" | "md" | "markdown" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn parse_query(q: &str) -> Vec<(String, String)> {
    q.split('&')
        .filter(|s| !s.is_empty())
        .map(|pair| {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            let dec = |s: &str| {
                percent_decode_str(&s.replace('+', " "))
                    .decode_utf8_lossy()
                    .into_owned()
            };
            (dec(k), dec(v))
        })
        .collect()
}

/// Paths are sent to the frontend as (lossy) strings.
fn ps(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

fn canon(p: &Path) -> PathBuf {
    dunce::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

fn list(dir: &str, all: bool) -> Res {
    let dir = canon(Path::new(dir));
    let rd = fs::read_dir(&dir).map_err(|e| (404, format!("{}: {e}", dir.display())))?;
    let mut entries: Vec<(bool, String, Value)> = rd
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                return None;
            }
            let path = e.path();
            // Follow symlinks so linked folders and files behave normally.
            let is_dir = fs::metadata(&path).map(|m| m.is_dir()).unwrap_or(false);
            let md = !is_dir && is_markdown(&path);
            if !is_dir && !md && !all {
                return None;
            }
            let v = json!({ "name": name, "path": ps(&path), "dir": is_dir, "md": md });
            Some((!is_dir, name.to_lowercase(), v))
        })
        .collect();
    entries.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
    Ok(json_resp(json!({
        "path": ps(&dir),
        "name": dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| dir.display().to_string()),
        "parent": dir.parent().map(ps),
        "entries": entries.into_iter().map(|e| e.2).collect::<Vec<_>>(),
    })))
}

fn read_text(path: &str) -> Result<(PathBuf, String), (u16, String)> {
    let p = canon(Path::new(path));
    let bytes = fs::read(&p).map_err(|e| (404, format!("{}: {e}", p.display())))?;
    Ok((p, String::from_utf8_lossy(&bytes).into_owned()))
}

fn render_file(path: &str) -> Res {
    let (p, text) = read_text(path)?;
    let html = render::markdown(&text);
    Ok(json_resp(json!({
        "path": ps(&p),
        "name": p.file_name().map(|n| n.to_string_lossy().into_owned()),
        "dir": p.parent().map(ps),
        "html": html,
        "lines": text.lines().count(),
    })))
}

fn source_file(path: &str) -> Res {
    let (p, text) = read_text(path)?;
    Ok(json_resp(json!({
        "path": ps(&p),
        "html": render::source(&text),
        "lines": text.lines().count().max(1),
    })))
}

/// Resolves `rel` (a link target from a document) against the document at `base`.
fn resolve_path(base: &str, rel: &str) -> PathBuf {
    let rel = percent_decode_str(rel).decode_utf8_lossy();
    let rel = rel.strip_prefix("file://").unwrap_or(&rel);
    let rel_path = Path::new(rel);
    if rel_path.is_absolute() {
        return rel_path.to_path_buf();
    }
    let base = Path::new(base);
    let dir = if base.is_dir() {
        base
    } else {
        base.parent().unwrap_or(base)
    };
    dir.join(rel_path)
}

fn resolve(base: &str, rel: &str) -> Value {
    let p = canon(&resolve_path(base, rel));
    let kind = match fs::metadata(&p) {
        Err(_) => "missing",
        Ok(m) if m.is_dir() => "dir",
        Ok(_) if is_markdown(&p) => "md",
        Ok(_) => "other",
    };
    json!({ "path": ps(&p), "kind": kind })
}

fn local_file(base: &str, rel: &str) -> Res {
    let p = resolve_path(base, rel);
    let data = fs::read(&p).map_err(|e| (404, format!("{}: {e}", p.display())))?;
    // Documents load images and media from here. Anything that could run code
    // if opened directly (HTML, SVG, scripts) is locked down by a sandbox CSP.
    let mime = match mime_for(&p.to_string_lossy()) {
        m if m.starts_with("text/html") || m.starts_with("text/javascript") => "text/plain",
        m => m,
    };
    let mut resp = respond(mime, Cow::Owned(data));
    resp.headers_mut().insert(
        "Content-Security-Policy",
        "sandbox; default-src 'none'; img-src 'self' data:; style-src 'unsafe-inline'"
            .parse()
            .unwrap(),
    );
    Ok(resp)
}

/// Whether a link target is a URL the OS may open (web or mail only).
pub fn is_web_url(url: &str) -> bool {
    let lower = url.trim_start().to_ascii_lowercase();
    ["http://", "https://", "mailto:"]
        .iter()
        .any(|s| lower.starts_with(s))
}

/// File types that are safe to hand to the OS when a document links to them.
/// Anything else (scripts, executables, installers...) is shown in its folder instead.
pub fn is_safe_to_open(p: &Path) -> bool {
    const SAFE: &[&str] = &[
        "png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "ico", "avif", "pdf", "txt", "log",
        "csv", "tsv", "json", "yaml", "yml", "toml", "xml", "ini", "cfg", "conf", "mp4", "webm",
        "mov", "mp3", "wav", "ogg", "flac",
    ];
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| SAFE.contains(&e.to_ascii_lowercase().as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_decoding() {
        let q = parse_query("path=%2Ftmp%2Fa%20b.md&all=1");
        assert_eq!(q[0], ("path".into(), "/tmp/a b.md".into()));
        assert_eq!(q[1], ("all".into(), "1".into()));
    }

    #[test]
    fn index_is_embedded() {
        assert!(ASSETS.iter().any(|(n, _)| *n == "index.html"));
        assert!(
            ASSETS
                .iter()
                .any(|(n, _)| *n == "vendor/katex/katex.min.css")
        );
    }

    #[test]
    fn open_policies() {
        assert!(is_web_url("https://example.com"));
        assert!(is_web_url("MAILTO:a@b.c"));
        assert!(!is_web_url("file:///etc/passwd"));
        assert!(!is_web_url("javascript:alert(1)"));
        assert!(is_safe_to_open(Path::new("/a/b.PNG")));
        assert!(!is_safe_to_open(Path::new("/a/setup.exe")));
        assert!(!is_safe_to_open(Path::new("/a/run.sh")));
        assert!(!is_safe_to_open(Path::new("/a/noext")));
    }

    #[test]
    fn resolves_relative() {
        let p = resolve_path("/a/b/readme.md", "../c/x.md");
        assert_eq!(p, Path::new("/a/b/../c/x.md"));
    }
}
