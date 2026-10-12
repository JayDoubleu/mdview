//! One window for every `mdview` launch, like `code file.md`: a later launch
//! hands its path to the running app and exits.
//!
//! The channel is private to the user and session: a Unix socket in a 0700
//! directory the user owns (the runtime dir on Linux, the cache dir
//! elsewhere), or a named pipe named after the user and session on Windows.
//! A message is the target's absolute path, exactly as the OS spells it,
//! ended by a NUL byte (which no path contains). The reply is a line, "ok" or
//! "error". Every step has a timeout, so a stuck instance can't hang a launch.

use interprocess::ConnectWaitMode;
use interprocess::local_socket::{
    ConnectOptions, GenericFilePath, Listener, ListenerOptions, Name, prelude::*,
};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Longest message accepted, to keep a misbehaving client from using memory.
const MAX_MESSAGE: u64 = 64 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(1);
const IO_TIMEOUT: Duration = Duration::from_secs(3);

/// Where the socket lives, or None if there's no safe place for it.
#[cfg(unix)]
fn socket_path() -> Option<PathBuf> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
    let dir = dirs::runtime_dir().or_else(dirs::cache_dir)?.join("mdview");
    let _ = std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&dir);
    // The folder may have existed already: it must be a real directory that
    // this user owns and nobody else can enter.
    let meta = std::fs::symlink_metadata(&dir).ok()?;
    // SAFETY: geteuid has no preconditions.
    if !meta.is_dir() || meta.uid() != unsafe { libc::geteuid() } {
        return None;
    }
    if meta.mode() & 0o077 != 0 {
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).ok()?;
    }
    Some(dir.join("instance.sock"))
}

#[cfg(windows)]
fn socket_path() -> Option<PathBuf> {
    unsafe extern "system" {
        fn GetCurrentProcessId() -> u32;
        fn ProcessIdToSessionId(process_id: u32, session_id: *mut u32) -> i32;
    }
    let mut session = 0u32;
    // SAFETY: both are plain Win32 calls; `session` outlives the call.
    if unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut session) } == 0 {
        return None;
    }
    // Hex keeps distinct accounts (j.doe / jdoe, DOMAIN\bob) distinct.
    let user = format!(
        "{}\\{}",
        std::env::var("USERDOMAIN").unwrap_or_default(),
        std::env::var("USERNAME").ok()?
    );
    let hex: String = user.bytes().map(|b| format!("{b:02x}")).collect();
    Some(PathBuf::from(format!(r"\\.\pipe\mdview-{hex}-{session}")))
}

fn name(path: &Path) -> Option<Name<'_>> {
    path.as_os_str().to_fs_name::<GenericFilePath>().ok()
}

fn encode(path: &Path) -> Option<Vec<u8>> {
    #[cfg(unix)]
    let mut bytes = std::os::unix::ffi::OsStrExt::as_bytes(path.as_os_str()).to_vec();
    // Windows paths that aren't valid Unicode can't round-trip; open them in
    // a window of their own instead.
    #[cfg(windows)]
    let mut bytes = path.to_str()?.as_bytes().to_vec();
    if bytes.contains(&0) {
        return None;
    }
    bytes.push(0);
    Some(bytes)
}

fn decode(mut bytes: Vec<u8>) -> Option<PathBuf> {
    if bytes.pop()? != 0 {
        return None; // cut off: no terminator
    }
    #[cfg(unix)]
    let path =
        PathBuf::from(<std::ffi::OsString as std::os::unix::ffi::OsStringExt>::from_vec(bytes));
    #[cfg(windows)]
    let path = PathBuf::from(String::from_utf8(bytes).ok()?);
    path.is_absolute().then_some(path)
}

/// Asks a running mdview to open `target`. True if one took it.
pub fn send(target: &Path) -> bool {
    let (Some(path), Some(msg)) = (socket_path(), encode(target)) else {
        return false;
    };
    let Some(name) = name(&path) else {
        return false;
    };
    let Ok(conn) = ConnectOptions::new()
        .name(name)
        .wait_mode(ConnectWaitMode::Timeout(CONNECT_TIMEOUT))
        .connect_sync()
    else {
        return false;
    };
    let _ = conn.set_recv_timeout(Some(IO_TIMEOUT));
    let _ = conn.set_send_timeout(Some(IO_TIMEOUT));
    let mut conn = BufReader::new(conn);
    if conn.get_mut().write_all(&msg).is_err() {
        return false;
    }
    let mut reply = String::new();
    conn.read_line(&mut reply).is_ok() && reply.trim() == "ok"
}

pub enum Bind {
    /// This process is the instance now; pass the listener to [`serve`].
    Listening(Listener),
    /// Another mdview is the instance.
    Running,
    /// No channel (no safe socket folder, or the name is held by something
    /// that isn't answering): just run without one.
    Unavailable,
}

/// Claims the instance name. Done before the window is built, so launches that
/// start at the same moment agree on a single winner.
pub fn bind() -> Bind {
    let Some(path) = socket_path() else {
        return Bind::Unavailable;
    };
    let Some(name) = name(&path) else {
        return Bind::Unavailable;
    };
    match ListenerOptions::new().name(name.clone()).create_sync() {
        Ok(l) => Bind::Listening(l),
        Err(e)
            if e.kind() == io::ErrorKind::AddrInUse
                || e.kind() == io::ErrorKind::PermissionDenied =>
        {
            let live = ConnectOptions::new()
                .name(name.clone())
                .wait_mode(ConnectWaitMode::Timeout(CONNECT_TIMEOUT))
                .connect_sync()
                .is_ok();
            if live {
                return Bind::Running;
            }
            // A socket file left behind by an instance that crashed: replace it.
            match ListenerOptions::new()
                .name(name)
                .try_overwrite(true)
                .create_sync()
            {
                Ok(l) => Bind::Listening(l),
                Err(_) => Bind::Unavailable,
            }
        }
        Err(_) => Bind::Unavailable,
    }
}

/// Answers later launches: `open` gets each path and says whether it was taken.
pub fn serve(listener: Listener, open: impl Fn(PathBuf) -> bool + Send + Sync + 'static) {
    let open = std::sync::Arc::new(open);
    std::thread::spawn(move || {
        for conn in listener.incoming().flatten() {
            // One thread per client, so a slow or silent one can't block others.
            let open = open.clone();
            std::thread::spawn(move || {
                let _ = conn.set_recv_timeout(Some(IO_TIMEOUT));
                let _ = conn.set_send_timeout(Some(IO_TIMEOUT));
                let mut reader = BufReader::new(conn).take(MAX_MESSAGE);
                let mut msg = Vec::new();
                let ok = reader.read_until(0, &mut msg).is_ok()
                    && decode(msg).is_some_and(|path| open(path));
                let mut conn = reader.into_inner().into_inner();
                let _ = conn.write_all(if ok { b"ok\n" } else { b"error\n" });
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_round_trip_exactly() {
        #[cfg_attr(windows, allow(unused_mut))] // Unix adds odd paths below
        let mut paths = vec![PathBuf::from(if cfg!(windows) {
            r"C:\a b\c.md"
        } else {
            "/a b/c.md"
        })];
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            paths.push(PathBuf::from("/tmp/a\nb.md"));
            paths.push(PathBuf::from("/tmp/x.md\r"));
            paths.push(PathBuf::from(std::ffi::OsStr::from_bytes(
                b"/tmp/caf\xe9.md",
            )));
        }
        for p in paths {
            assert_eq!(decode(encode(&p).unwrap()), Some(p.clone()), "{p:?}");
        }
        // A message cut short (no terminator) or a relative path is refused.
        let mut cut = encode(Path::new(if cfg!(windows) { r"C:\x.md" } else { "/x.md" })).unwrap();
        cut.pop();
        assert_eq!(decode(cut), None);
        assert_eq!(decode(b"rel.md\0".to_vec()), None);
    }
}
