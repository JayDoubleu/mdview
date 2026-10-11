//! Handler for the `mdview://` custom protocol: static assets plus a tiny JSON API
//! that the frontend uses to list directories, render files and load local images.

use crate::render;
use crate::text;
use percent_encoding::percent_decode_str;
use serde_json::{Value, json};
use std::borrow::Cow;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
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
        "/api/counts" => folder_counts(q("path").unwrap_or(".")),
        "/api/render" => render_file(q("path").unwrap_or("")),
        "/api/source" => source_file(q("path").unwrap_or("")),
        "/api/raw" => raw_file(q("path").unwrap_or("")),
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

/// Requests that can take a while (walking folder trees) and should be
/// answered off the UI thread.
pub fn is_slow(req: &Request<Vec<u8>>) -> bool {
    req.uri().path() == "/api/counts"
}

/// How many entries to look at per folder before giving up and reporting "N+".
const COUNT_BUDGET: usize = 20_000;

/// Markdown file counts (including subfolders) for each folder inside `dir`.
fn folder_counts(dir: &str) -> Res {
    // A few requests at a time at most; the page simply skips the badges otherwise.
    static RUNNING: AtomicUsize = AtomicUsize::new(0);
    struct Slot;
    impl Drop for Slot {
        fn drop(&mut self) {
            RUNNING.fetch_sub(1, Ordering::SeqCst);
        }
    }
    if RUNNING.fetch_add(1, Ordering::SeqCst) >= 4 {
        RUNNING.fetch_sub(1, Ordering::SeqCst);
        return Err((503, "busy".into()));
    }
    let _slot = Slot;
    let mut budget = COUNT_BUDGET * 5;
    let dir = canon(Path::new(dir));
    let rd = fs::read_dir(&dir).map_err(|e| (404, format!("{}: {e}", dir.display())))?;
    let mut counts = serde_json::Map::new();
    for e in rd.flatten() {
        if e.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        // Symlinked folders aren't followed (they may point at / or $HOME).
        if !e.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        let path = e.path();
        // Each folder gets up to COUNT_BUDGET entries, all of them together 5×.
        let mut share = budget.min(COUNT_BUDGET);
        let before = share;
        let (n, complete) = count_markdown(&path, &mut share);
        budget -= before - share;
        counts.insert(ps(&path), json!({ "n": n, "partial": !complete }));
    }
    Ok(json_resp(json!({ "path": ps(&dir), "counts": counts })))
}

/// Counts Markdown files under `dir`, skipping hidden entries and not following
/// symlinked folders. Returns `(count, complete)`; incomplete if the budget
/// (entries to look at, shared by the caller) ran out.
pub fn count_markdown(dir: &Path, budget: &mut usize) -> (usize, bool) {
    let mut stack = vec![dir.to_path_buf()];
    let mut n = 0;
    while let Some(d) = stack.pop() {
        let Ok(rd) = fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            if *budget == 0 {
                return (n, false);
            }
            *budget -= 1;
            if e.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() {
                stack.push(e.path());
            } else if is_markdown(&e.path()) {
                n += 1;
            }
        }
    }
    (n, true)
}

/// The file last handed to the editor; the only one `save` may write.
static EDITABLE: Mutex<Option<PathBuf>> = Mutex::new(None);

fn raw_file(path: &str) -> Res {
    let p = canon(Path::new(path));
    let bytes = fs::read(&p).map_err(|e| (404, format!("{}: {e}", p.display())))?;
    let (text, enc, lossy) = text::decode_checked(&bytes);
    *EDITABLE.lock().unwrap_or_else(|e| e.into_inner()) = Some(p);
    let mut r = respond("text/plain; charset=utf-8", Cow::Owned(text.into_bytes()));
    let h = r.headers_mut();
    h.insert("X-Mdview-Encoding", enc.label().parse().unwrap());
    if lossy {
        h.insert("X-Mdview-Lossy", "1".parse().unwrap());
    }
    Ok(r)
}

/// Whether `p` is the file the editor loaded (checked before saving, so the
/// page can only ever write back the file it is editing).
pub fn is_editable(p: &Path) -> bool {
    EDITABLE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_deref()
        .is_some_and(|e| e == canon(p))
}

fn read_text(path: &str) -> Result<(PathBuf, String), (u16, String)> {
    let (p, text, _) = read_text_enc(path)?;
    Ok((p, text))
}

fn read_text_enc(path: &str) -> Result<(PathBuf, String, text::Encoding), (u16, String)> {
    let p = canon(Path::new(path));
    let bytes = fs::read(&p).map_err(|e| (404, format!("{}: {e}", p.display())))?;
    let (text, enc) = text::decode(&bytes);
    Ok((p, text, enc))
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
    let base = Path::new(base);
    let dir = if base.is_dir() {
        base
    } else {
        base.parent().unwrap_or(base)
    };
    if rel.starts_with(['/', '\\']) {
        if rel_path.is_absolute() && rel_path.exists() {
            return rel_path.to_path_buf();
        }
        // Static-site paths like /uploads/a.png are relative to the site root,
        // which is an ancestor folder (or its static/ or public/ folder).
        let trimmed = rel.trim_start_matches(['/', '\\']);
        for ancestor in dir.ancestors() {
            for sub in ["", "static", "public"] {
                let candidate = ancestor.join(sub).join(trimmed);
                if candidate.exists() {
                    return candidate;
                }
            }
        }
        return rel_path.to_path_buf();
    }
    if rel_path.is_absolute() {
        return rel_path.to_path_buf();
    }
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

/// Saves edited text over an existing Markdown file, keeping its line endings
/// and encoding. Only Markdown files that already exist can be written.
/// Returns a note for the user when the file couldn't be kept as it was.
pub fn save_file(p: &Path, new_text: &str) -> Result<Option<String>, String> {
    let meta = fs::metadata(p).map_err(|e| format!("{}: {e}", p.display()))?;
    if !meta.is_file() || !is_markdown(p) {
        return Err(format!("{} is not a Markdown file", p.display()));
    }
    let (old, enc, lossy) = text::decode_checked(&fs::read(p).unwrap_or_default());
    if lossy {
        return Err(format!(
            "{} contains bytes that aren't valid {}. Saving would damage them, so it was not saved.",
            p.display(),
            enc.label()
        ));
    }
    let new_text = new_text.replace("\r\n", "\n");
    let body = if old.contains("\r\n") {
        new_text.replace('\n', "\r\n")
    } else {
        new_text
    };
    let (bytes, note) = match text::encode(&body, enc) {
        Some(b) => (b, None),
        None => (
            body.into_bytes(),
            Some(format!(
                "Saved as UTF-8: the text has characters {} can't store",
                enc.label()
            )),
        ),
    };
    fs::write(p, bytes).map_err(|e| format!("{}: {e}", p.display()))?;
    Ok(note)
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
    fn save_keeps_line_endings_and_refuses_non_markdown() {
        let dir = std::env::temp_dir().join(format!("mdview-save-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let crlf = dir.join("crlf.md");
        fs::write(&crlf, "a\r\nb\r\n").unwrap();
        save_file(&crlf, "x\ny\n").unwrap();
        assert_eq!(fs::read_to_string(&crlf).unwrap(), "x\r\ny\r\n");

        let lf = dir.join("lf.md");
        fs::write(&lf, "a\n").unwrap();
        save_file(&lf, "z\n").unwrap();
        assert_eq!(fs::read_to_string(&lf).unwrap(), "z\n");

        let txt = dir.join("notes.txt");
        fs::write(&txt, "keep").unwrap();
        assert!(save_file(&txt, "nope").is_err());
        assert_eq!(fs::read_to_string(&txt).unwrap(), "keep");
        assert!(save_file(&dir.join("missing.md"), "x").is_err());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn counts_markdown_recursively() {
        let dir = std::env::temp_dir().join(format!("mdview-count-{}", std::process::id()));
        fs::create_dir_all(dir.join("a/b")).unwrap();
        fs::create_dir_all(dir.join(".hidden")).unwrap();
        fs::create_dir_all(dir.join("empty")).unwrap();
        for f in [
            "x.md",
            "a/y.markdown",
            "a/b/z.md",
            "a/notes.txt",
            ".hidden/h.md",
        ] {
            fs::write(dir.join(f), "#").unwrap();
        }
        assert_eq!(count_markdown(&dir, &mut 1000), (3, true));
        assert_eq!(count_markdown(&dir.join("empty"), &mut 1000), (0, true));
        assert!(!count_markdown(&dir, &mut 2).1);
        fs::remove_dir_all(&dir).unwrap();
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
    fn resolves_site_root_paths() {
        let dir = std::env::temp_dir().join(format!("mdview-site-{}", std::process::id()));
        fs::create_dir_all(dir.join("static/uploads")).unwrap();
        fs::create_dir_all(dir.join("content/posts")).unwrap();
        fs::write(dir.join("static/uploads/a.png"), "x").unwrap();
        let post = dir.join("content/posts/p.md");
        fs::write(&post, "x").unwrap();
        let found = resolve_path(&post.to_string_lossy(), "/uploads/a.png");
        assert_eq!(found, dir.join("static/uploads/a.png"));
        // Real absolute paths still win.
        let abs = dir.join("content/posts/p.md");
        assert_eq!(resolve_path("/elsewhere/x.md", &abs.to_string_lossy()), abs);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn save_keeps_encoding() {
        let dir = std::env::temp_dir().join(format!("mdview-enc-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let f = dir.join("old.md");
        fs::write(&f, b"caf\xe9\r\n").unwrap();
        assert_eq!(save_file(&f, "café crème\n").unwrap(), None);
        assert_eq!(fs::read(&f).unwrap(), b"caf\xe9 cr\xe8me\r\n");
        // Characters Windows-1252 can't hold fall back to UTF-8, with a note.
        assert!(save_file(&f, "emoji 😀\n").unwrap().is_some());
        assert_eq!(fs::read_to_string(&f).unwrap(), "emoji 😀\r\n");
        // A UTF-8 file with a damaged byte is left alone rather than rewritten.
        let damaged = b"caf\xc3\xa9 \x92s\n";
        fs::write(&f, damaged).unwrap();
        assert!(save_file(&f, "anything\n").is_err());
        assert_eq!(fs::read(&f).unwrap(), damaged);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn resolves_relative() {
        let p = resolve_path("/a/b/readme.md", "../c/x.md");
        assert_eq!(p, Path::new("/a/b/../c/x.md"));
    }
}
