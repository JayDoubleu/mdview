//! One window for every `mdview` launch, like `code file.md`: a later launch
//! hands its path to the running app and exits.
//!
//! The channel is private to the user: a Unix socket in a 0700 directory
//! (the runtime dir on Linux, the cache dir elsewhere), or a named pipe on
//! Windows. Messages are a single line holding an absolute path.

use interprocess::local_socket::{GenericFilePath, ListenerOptions, Name, Stream, prelude::*};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};

/// Longest message accepted, to keep a misbehaving client from using memory.
const MAX_MESSAGE: u64 = 64 * 1024;

#[cfg(unix)]
fn socket_path() -> Option<PathBuf> {
    use std::os::unix::fs::DirBuilderExt;
    let dir = dirs::runtime_dir().or_else(dirs::cache_dir)?.join("mdview");
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&dir)
        .ok()?;
    Some(dir.join("instance.sock"))
}

#[cfg(windows)]
fn socket_path() -> Option<PathBuf> {
    let user = std::env::var("USERNAME").unwrap_or_default();
    let user: String = user.chars().filter(|c| c.is_alphanumeric()).collect();
    Some(PathBuf::from(format!(r"\\.\pipe\mdview-{user}")))
}

fn name(path: &Path) -> Option<Name<'_>> {
    path.as_os_str().to_fs_name::<GenericFilePath>().ok()
}

/// Asks a running mdview to open `target`. True if one took it.
pub fn send(target: &Path) -> bool {
    let Some(path) = socket_path() else {
        return false;
    };
    let Some(name) = name(&path) else {
        return false;
    };
    let Ok(mut conn) = Stream::connect(name) else {
        return false;
    };
    let msg = format!("{}\n", target.to_string_lossy());
    if conn.write_all(msg.as_bytes()).is_err() {
        return false;
    }
    let mut reply = String::new();
    BufReader::new(conn).read_line(&mut reply).is_ok() && reply.trim() == "ok"
}

/// Becomes the running instance: calls `open` with each path a later launch sends.
/// Does nothing if another instance already listens (e.g. `--new-window`).
pub fn listen(open: impl Fn(PathBuf) + Send + 'static) {
    let Some(path) = socket_path() else { return };
    let Some(name) = name(&path) else { return };
    let listener = match ListenerOptions::new().name(name.clone()).create_sync() {
        Ok(l) => l,
        // The name is taken. If nothing answers on it, it's a socket file left
        // behind by an instance that crashed: replace it. Otherwise another
        // window is the instance and keeps that role.
        Err(e)
            if e.kind() == std::io::ErrorKind::AddrInUse
                && Stream::connect(name.clone()).is_err() =>
        {
            match ListenerOptions::new()
                .name(name)
                .try_overwrite(true)
                .create_sync()
            {
                Ok(l) => l,
                Err(_) => return,
            }
        }
        Err(_) => return,
    };
    std::thread::spawn(move || {
        for conn in listener.incoming().flatten() {
            let mut reader = BufReader::new(conn).take(MAX_MESSAGE);
            let mut line = String::new();
            if reader.read_line(&mut line).is_err() {
                continue;
            }
            let target = PathBuf::from(line.trim_end_matches(['\r', '\n']));
            let mut conn = reader.into_inner().into_inner();
            if target.is_absolute() {
                let _ = conn.write_all(b"ok\n");
                open(target);
            } else {
                let _ = conn.write_all(b"error\n");
            }
        }
    });
}
