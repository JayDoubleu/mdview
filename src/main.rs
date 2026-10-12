// Hide the console window for release builds on Windows.
#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

mod annotations;
mod icon;
mod instance;
mod render;
mod server;
mod text;

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use serde_json::{Value, json};
use server::Launch;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tao::dpi::LogicalSize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tao::window::WindowBuilder;
use wry::{NewWindowResponse, WebContext, WebViewBuilder};

const USAGE: &str = "mdview - a lightweight Markdown viewer

USAGE:
    mdview [PATH]

ARGS:
    PATH    Markdown file or directory to open (defaults to the current directory)

OPTIONS:
    -n, --new-window Open a new window instead of using the one already open
    -w, --wait       Keep the terminal attached until the window is closed
                     (always opens a new window)
    -h, --help       Print help
    -V, --version    Print version

KEYS (Cmd on macOS):
    Ctrl+B  toggle file tree        Ctrl+E  toggle source pane
    Ctrl+R  reload                  Alt+Left/Right  back / forward
    Ctrl+=/-/0  zoom                Ctrl+L  toggle light / dark
    Ctrl+F  find in document        Backspace  tree: go up one folder";

enum UserEvent {
    Title(String),
    Watch(Vec<PathBuf>),
    FsChanged(Vec<PathBuf>),
    Dropped(Vec<PathBuf>),
    /// Another `mdview` launch asked this window to open a path.
    Open(PathBuf),
    /// The page finished starting up and can take `Open`s.
    PageReady,
    /// The page has (or no longer has) unsaved edits.
    Dirty(bool),
    /// Answer to a request from the page, delivered to `mdview.reply`.
    Reply(Value),
    /// Close the window, asking the page first if there are unsaved edits.
    CloseRequested,
    CloseAck,
    Quit,
}

/// Release builds on Windows have no console; borrow the parent's so output is visible.
fn attach_console() {
    #[cfg(windows)]
    {
        unsafe extern "system" {
            fn AttachConsole(process_id: u32) -> i32;
        }
        // ATTACH_PARENT_PROCESS
        unsafe { AttachConsole(u32::MAX) };
    }
}

struct Args {
    /// Absolute path to open (file or folder).
    target: PathBuf,
    wait: bool,
    new_window: bool,
    /// This is the copy `detach` started in the background.
    background: bool,
}

fn parse_args() -> Args {
    let mut path: Option<PathBuf> = None;
    let mut wait = false;
    let mut new_window = false;
    let mut background = false;
    for arg in std::env::args_os().skip(1) {
        match arg.to_str() {
            Some("-h" | "--help") => {
                attach_console();
                println!("{USAGE}");
                std::process::exit(0);
            }
            Some("-V" | "--version") => {
                attach_console();
                println!("mdview {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            Some("-w" | "--wait") => wait = true,
            Some("-n" | "--new-window") => new_window = true,
            Some(BACKGROUND_ARG) => background = true,
            Some(flag) if flag.starts_with('-') && flag.len() > 1 && !Path::new(flag).exists() => {
                attach_console();
                eprintln!("mdview: unknown option '{flag}'\n\n{USAGE}");
                std::process::exit(2);
            }
            _ if path.is_none() => path = Some(PathBuf::from(arg)),
            _ => {}
        }
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let target = path
        .map(|p| if p.is_absolute() { p } else { cwd.join(p) })
        .unwrap_or(cwd);
    Args {
        target,
        wait,
        new_window,
        background,
    }
}

/// What to show for `target`: a file and its folder, or a folder and its README.
fn launch_for(target: &Path) -> Launch {
    let canon = |p: &Path| dunce::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    if target.is_dir() {
        let root = canon(target);
        let file = ["README.md", "readme.md", "Readme.md", "index.md"]
            .iter()
            .map(|n| root.join(n))
            .find(|p| p.is_file());
        Launch {
            root,
            file,
            error: None,
        }
    } else if target.is_file() {
        let file = canon(target);
        let root = file
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| file.clone());
        Launch {
            root,
            file: Some(file),
            error: None,
        }
    } else {
        let root = target
            .parent()
            .filter(|p| p.is_dir())
            .map(canon)
            .unwrap_or_else(|| canon(Path::new(".")));
        Launch {
            root,
            file: None,
            error: Some(format!("File not found: {}", target.display())),
        }
    }
}

/// Passed to the copy that `detach` starts in the background. An argument,
/// not an environment variable, so programs mdview opens don't inherit it.
const BACKGROUND_ARG: &str = "--mdview-background";

/// Like `code file.md`: relaunch in the background so the terminal is free again.
/// Windows release builds are GUI apps, which shells don't wait for anyway.
#[cfg(unix)]
fn detach() -> bool {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    Command::new(exe)
        .args(std::env::args_os().skip(1))
        .arg(BACKGROUND_ARG)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .is_ok()
}

fn main() -> wry::Result<()> {
    let Args {
        target,
        wait,
        new_window,
        background,
    } = parse_args();
    let launch = launch_for(&target);
    if let Some(error) = &launch.error {
        if !background {
            attach_console();
            eprintln!("mdview: {error}");
        }
    }
    // Hand the path to the window that's already open, if there is one.
    let handoff = !wait && !new_window && launch.error.is_none();
    if handoff && !background && instance::send(&target) {
        return Ok(());
    }
    #[cfg(unix)]
    if !wait && !background && detach() {
        return Ok(());
    }
    // Become the instance (a `--wait` window doesn't, since navigating it away
    // would end the wait). Claimed before the window is built, so launches at
    // the same moment end up in one window; whoever loses hands over.
    let listener = if wait {
        None
    } else {
        match instance::bind() {
            instance::Bind::Listening(l) => Some(l),
            instance::Bind::Running if handoff && instance::send(&target) => return Ok(()),
            _ => None,
        }
    };
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    webkitgtk_env();
    let launch = Arc::new(launch);

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    let window = WindowBuilder::new()
        .with_title("mdview")
        .with_inner_size(LogicalSize::new(1280.0, 860.0))
        .with_min_inner_size(LogicalSize::new(480.0, 320.0))
        .with_window_icon(icon::window_icon())
        .build(&event_loop)
        .expect("failed to create window");
    icon::install();

    // Later launches (`mdview other.md`) open here instead of in a new window.
    if let Some(listener) = listener {
        let open_proxy = proxy.clone();
        instance::serve(listener, move |path| {
            open_proxy.send_event(UserEvent::Open(path)).is_ok()
        });
    }

    // Custom protocol origins differ per platform (see wry docs).
    #[cfg(any(target_os = "windows", target_os = "android"))]
    let origin = "http://mdview.localhost";
    #[cfg(not(any(target_os = "windows", target_os = "android")))]
    let origin = "mdview://localhost";

    let ipc_proxy = proxy.clone();
    let drop_proxy = proxy.clone();
    let proto_launch = launch.clone();

    // Keep webview data (settings in localStorage) in a per-user location.
    // WebView2 would otherwise try to write next to the executable.
    let mut web_context = WebContext::new(dirs::data_local_dir().map(|d| d.join("mdview")));

    #[cfg(target_os = "macos")]
    let _menu = macos_menu(proxy.clone());

    let builder = WebViewBuilder::new_with_web_context(&mut web_context)
        .with_asynchronous_custom_protocol("mdview".into(), move |_id, req, responder| {
            // Folder counting walks directory trees; keep it off the UI thread.
            if server::is_slow(&req) {
                let launch = proto_launch.clone();
                std::thread::spawn(move || responder.respond(server::handle(&launch, req)));
            } else {
                responder.respond(server::handle(&proto_launch, req));
            }
        })
        .with_url(format!("{origin}/"))
        .with_devtools(cfg!(debug_assertions))
        .with_ipc_handler(move |req| handle_ipc(&ipc_proxy, req.body()))
        .with_navigation_handler(move |url| {
            // The app is a single page; never navigate away from it.
            let page = url.split(['?', '#']).next().unwrap_or("");
            if page == format!("{origin}/") || page == format!("{origin}/index.html") {
                true
            } else {
                if server::is_web_url(&url) {
                    let _ = open::that_detached(&url);
                }
                false
            }
        })
        .with_new_window_req_handler(|url, _| {
            if server::is_web_url(&url) {
                let _ = open::that_detached(&url);
            }
            NewWindowResponse::Deny
        })
        .with_drag_drop_handler(move |ev| {
            if let wry::DragDropEvent::Drop { paths, .. } = ev {
                let _ = drop_proxy.send_event(UserEvent::Dropped(paths));
                return true;
            }
            false
        });

    #[cfg(any(target_os = "windows", target_os = "macos"))]
    let webview = builder.build(&window)?;
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let webview = {
        use tao::platform::unix::WindowExtUnix;
        use wry::WebViewBuilderExtUnix;
        let webview = builder.build_gtk(window.default_vbox().expect("no gtk vbox"))?;
        tune_webkitgtk(&webview);
        webview
    };

    let mut watcher = make_watcher(proxy.clone());
    // Folders shown in the tree plus the open file's folder.
    let mut watched: Vec<PathBuf> = Vec::new();

    let mut dirty = false;
    let mut page_ready = false;
    let mut pending: Vec<PathBuf> = Vec::new();
    let mut close_asked: Option<(Instant, bool)> = None;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            }
            | Event::UserEvent(UserEvent::CloseRequested) => {
                // With unsaved edits the page asks first and replies with Quit.
                // If it never even acknowledged the last request (it hung),
                // closing again quits anyway.
                let hung = close_asked.is_some_and(|(t, acked): (Instant, bool)| {
                    !acked && t.elapsed() > Duration::from_secs(2)
                });
                if dirty && !hung {
                    send_js(&webview, "requestClose", Value::Null);
                    close_asked = Some((Instant::now(), false));
                } else {
                    *control_flow = ControlFlow::Exit;
                }
            }
            Event::UserEvent(UserEvent::CloseAck) => {
                if let Some((_, acked)) = close_asked.as_mut() {
                    *acked = true;
                }
            }
            Event::UserEvent(UserEvent::Quit) => *control_flow = ControlFlow::Exit,
            Event::UserEvent(UserEvent::Dirty(d)) => dirty = d,
            Event::UserEvent(UserEvent::Reply(v)) => send_js(&webview, "reply", v),
            Event::UserEvent(UserEvent::Title(t)) => window.set_title(&t),
            Event::UserEvent(UserEvent::Watch(dirs)) => {
                if let Some(w) = watcher.as_mut() {
                    watched.retain(|d| {
                        // A deleted folder lost its watch; drop it so it is
                        // watched again if it comes back.
                        let keep = dirs.contains(d) && d.is_dir();
                        if !keep {
                            let _ = w.unwatch(d);
                        }
                        keep
                    });
                    for dir in dirs {
                        if !watched.contains(&dir)
                            && w.watch(&dir, RecursiveMode::NonRecursive).is_ok()
                        {
                            watched.push(dir);
                        }
                    }
                }
            }
            Event::UserEvent(UserEvent::FsChanged(paths)) => {
                let paths: Vec<_> = paths.iter().map(|p| p.to_string_lossy()).collect();
                send_js(&webview, "fsChanged", json!(paths));
            }
            // Paths that arrive before the page is ready wait for it.
            Event::UserEvent(UserEvent::Open(path)) if !page_ready => pending.push(path),
            Event::UserEvent(UserEvent::PageReady) => {
                page_ready = true;
                for path in pending.drain(..) {
                    let _ = proxy.send_event(UserEvent::Open(path));
                }
            }
            Event::UserEvent(UserEvent::Open(path)) => {
                let l = launch_for(&path);
                send_js(
                    &webview,
                    "openLaunch",
                    json!({
                        "root": l.root.to_string_lossy(),
                        "file": l.file.map(|f| f.to_string_lossy().into_owned()),
                        "dir": path.is_dir(),
                        "error": l.error,
                    }),
                );
                window.set_minimized(false);
                window.set_visible(true);
                raise(&window);
            }
            Event::UserEvent(UserEvent::Dropped(paths)) => {
                if let Some(p) = paths.first() {
                    send_js(&webview, "openPath", json!(p.to_string_lossy()));
                }
            }
            _ => {}
        }
    });
}

/// WebKitGTK's GPU (DMA-BUF) renderer stutters badly with NVIDIA's driver.
/// Must run before WebKit starts; an explicit setting from the user wins.
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn webkitgtk_env() {
    const VAR: &str = "WEBKIT_DISABLE_DMABUF_RENDERER";
    if std::env::var_os(VAR).is_none() && Path::new("/proc/driver/nvidia/version").exists() {
        // SAFETY: called at startup before any other threads exist.
        unsafe { std::env::set_var(VAR, "1") };
    }
}

/// WebKitGTK animates wheel scrolling, which feels laggy next to native apps.
/// Scroll immediately instead unless MDVIEW_SMOOTH_SCROLL is set.
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn tune_webkitgtk(webview: &wry::WebView) {
    use webkit2gtk::{SettingsExt, WebViewExt};
    use wry::WebViewExtUnix;
    if let Some(settings) = WebViewExt::settings(&webview.webview()) {
        settings.set_enable_smooth_scrolling(std::env::var_os("MDVIEW_SMOOTH_SCROLL").is_some());
    }
}

/// macOS needs an application menu for standard shortcuts (Cmd+C, Cmd+A, Cmd+Q...) to work.
#[cfg(target_os = "macos")]
fn macos_menu(proxy: EventLoopProxy<UserEvent>) -> muda::Menu {
    use muda::accelerator::{Accelerator, Code, Modifiers};
    use muda::{Menu, MenuEvent, MenuItem, PredefinedMenuItem as P, Submenu};
    // A custom Quit (instead of the predefined one, which terminates at once)
    // so unsaved edits get the same prompt as closing the window.
    let quit = MenuItem::with_id(
        "quit",
        "Quit mdview",
        true,
        Some(Accelerator::new(Modifiers::META, Code::KeyQ)),
    );
    MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
        if e.id == "quit" {
            let _ = proxy.send_event(UserEvent::CloseRequested);
        }
    }));
    let app = Submenu::with_items(
        "mdview",
        true,
        &[
            &P::about(None, None),
            &P::separator(),
            &P::hide(None),
            &P::hide_others(None),
            &P::show_all(None),
            &P::separator(),
            &quit,
        ],
    )
    .expect("menu");
    let edit = Submenu::with_items(
        "Edit",
        true,
        &[
            &P::undo(None),
            &P::redo(None),
            &P::separator(),
            &P::cut(None),
            &P::copy(None),
            &P::paste(None),
            &P::select_all(None),
        ],
    )
    .expect("menu");
    let window = Submenu::with_items(
        "Window",
        true,
        &[
            &P::minimize(None),
            &P::maximize(None),
            &P::fullscreen(None),
            &P::separator(),
            &P::close_window(None),
        ],
    )
    .expect("menu");
    let menu = Menu::with_items(&[&app, &edit, &window]).expect("menu");
    menu.init_for_nsapp();
    menu
}

/// Brings the window to the front. GTK's present() also un-minimizes, which
/// tao's set_focus skips while the window is still minimized.
fn raise(window: &tao::window::Window) {
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        use gtk::prelude::GtkWindowExt;
        use tao::platform::unix::WindowExtUnix;
        window.gtk_window().present();
    }
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    window.set_focus();
}

fn send_js(webview: &wry::WebView, func: &str, arg: Value) {
    let _ = webview.evaluate_script(&format!("window.mdview && window.mdview.{func}({arg})"));
}

fn handle_ipc(proxy: &EventLoopProxy<UserEvent>, body: &str) {
    let Ok(msg) = serde_json::from_str::<Value>(body) else {
        return;
    };
    let arg = msg.get("arg").and_then(Value::as_str).unwrap_or("");
    match msg.get("cmd").and_then(Value::as_str) {
        Some("title") => {
            let _ = proxy.send_event(UserEvent::Title(arg.to_string()));
        }
        Some("watch") => {
            let dirs: Vec<PathBuf> = serde_json::from_str::<Vec<String>>(arg)
                .unwrap_or_default()
                .into_iter()
                .take(MAX_WATCHED)
                .map(PathBuf::from)
                .collect();
            let _ = proxy.send_event(UserEvent::Watch(dirs));
        }
        // Links in documents: web and mail URLs only.
        Some("open_url") if server::is_web_url(arg) => {
            let _ = open::that_detached(arg);
        }
        // Local files linked from documents: open harmless types, otherwise
        // just show the folder so nothing executable is ever launched.
        Some("open_link_path") => {
            let p = Path::new(arg);
            if p.is_dir() || (p.is_file() && server::is_safe_to_open(p)) {
                let _ = open::that_detached(p);
            } else if let Some(dir) = p.parent().filter(|d| d.is_dir()) {
                let _ = open::that_detached(dir);
            }
        }
        // Files the user double-clicks in the tree.
        Some("open_path") if Path::new(arg).exists() => {
            let _ = open::that_detached(arg);
        }
        Some("close") => {
            let _ = proxy.send_event(UserEvent::CloseRequested);
        }
        Some("ready") => {
            let _ = proxy.send_event(UserEvent::PageReady);
        }
        Some("close_ack") => {
            let _ = proxy.send_event(UserEvent::CloseAck);
        }
        Some("quit") => {
            let _ = proxy.send_event(UserEvent::Quit);
        }
        Some("dirty") => {
            let _ = proxy.send_event(UserEvent::Dirty(arg == "1"));
        }
        // Edit mode: render the unsaved buffer for the live preview.
        Some("render_text") => {
            let html = render::markdown(arg);
            reply(proxy, &msg, json!({ "html": html }));
        }
        Some("save") => {
            let path = msg.get("path").and_then(Value::as_str).unwrap_or("");
            let saved = if server::is_editable(Path::new(path)) {
                server::save_file(Path::new(path), arg)
            } else {
                Err("That file isn't open for editing".into())
            };
            let result = match saved {
                Ok(note) => json!({ "ok": true, "note": note }),
                Err(e) => json!({ "ok": false, "error": e }),
            };
            reply(proxy, &msg, result);
        }
        _ => {}
    }
}

fn reply(proxy: &EventLoopProxy<UserEvent>, msg: &Value, data: Value) {
    let id = msg.get("id").cloned().unwrap_or(Value::Null);
    let _ = proxy.send_event(UserEvent::Reply(json!({ "id": id, "data": data })));
}

/// Expanding many folders shouldn't exhaust the OS's watch handles.
const MAX_WATCHED: usize = 64;

fn make_watcher(proxy: EventLoopProxy<UserEvent>) -> Option<RecommendedWatcher> {
    notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(ev) = res {
            if matches!(ev.kind, notify::EventKind::Access(_)) {
                return;
            }
            let _ = proxy.send_event(UserEvent::FsChanged(ev.paths));
        }
    })
    .ok()
}
