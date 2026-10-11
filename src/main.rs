// Hide the console window for release builds on Windows.
#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

mod render;
mod server;
mod text;

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use serde_json::{Value, json};
use server::Launch;
use std::path::{Path, PathBuf};
use std::sync::Arc;
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
    -w, --wait       Keep the terminal attached until the window is closed
    -h, --help       Print help
    -V, --version    Print version

KEYS (Cmd on macOS):
    Ctrl+B  toggle file tree        Ctrl+E  toggle source pane
    Ctrl+R  reload                  Alt+Left/Right  back / forward
    Ctrl+=/-/0  zoom                Ctrl+L  toggle light / dark
    Ctrl+F  find in document        Backspace  tree: go up one folder";

enum UserEvent {
    Title(String),
    Watch(PathBuf),
    FsChanged(Vec<PathBuf>),
    Dropped(Vec<PathBuf>),
    /// The page has (or no longer has) unsaved edits.
    Dirty(bool),
    /// Answer to a request from the page, delivered to `mdview.reply`.
    Reply(Value),
    /// Close the window, asking the page first if there are unsaved edits.
    CloseRequested,
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

fn parse_args() -> (Launch, bool) {
    let mut path: Option<PathBuf> = None;
    let mut wait = false;
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
        .unwrap_or(cwd.clone());
    let canon = |p: &Path| dunce::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());

    if target.is_dir() {
        let root = canon(&target);
        let file = ["README.md", "readme.md", "Readme.md", "index.md"]
            .iter()
            .map(|n| root.join(n))
            .find(|p| p.is_file());
        (
            Launch {
                root,
                file,
                error: None,
            },
            wait,
        )
    } else if target.is_file() {
        let file = canon(&target);
        let root = file.parent().map(Path::to_path_buf).unwrap_or(cwd);
        (
            Launch {
                root,
                file: Some(file),
                error: None,
            },
            wait,
        )
    } else {
        let root = target
            .parent()
            .filter(|p| p.is_dir())
            .map(canon)
            .unwrap_or_else(|| canon(&cwd));
        let error = Some(format!("File not found: {}", target.display()));
        attach_console();
        eprintln!("mdview: {}", error.as_deref().unwrap());
        (
            Launch {
                root,
                file: None,
                error,
            },
            wait,
        )
    }
}

/// Like `code file.md`: relaunch in the background so the terminal is free again.
/// Windows release builds are GUI apps, which shells don't wait for anyway.
#[cfg(unix)]
fn detach() -> bool {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    const CHILD_ENV: &str = "MDVIEW_DETACHED";
    if std::env::var_os(CHILD_ENV).is_some() {
        return false;
    }
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    Command::new(exe)
        .args(std::env::args_os().skip(1))
        .env(CHILD_ENV, "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .is_ok()
}

fn main() -> wry::Result<()> {
    let (launch, wait) = parse_args();
    #[cfg(unix)]
    if !wait && detach() {
        return Ok(());
    }
    #[cfg(not(unix))]
    let _ = wait;
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    webkitgtk_env();
    let launch = Arc::new(launch);

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    let window = WindowBuilder::new()
        .with_title("mdview")
        .with_inner_size(LogicalSize::new(1280.0, 860.0))
        .with_min_inner_size(LogicalSize::new(480.0, 320.0))
        .build(&event_loop)
        .expect("failed to create window");

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
    let mut watched: Option<PathBuf> = None;

    let mut dirty = false;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            }
            | Event::UserEvent(UserEvent::CloseRequested) => {
                // With unsaved edits the page asks first and replies with Quit.
                if dirty {
                    send_js(&webview, "requestClose", Value::Null);
                } else {
                    *control_flow = ControlFlow::Exit;
                }
            }
            Event::UserEvent(UserEvent::Quit) => *control_flow = ControlFlow::Exit,
            Event::UserEvent(UserEvent::Dirty(d)) => dirty = d,
            Event::UserEvent(UserEvent::Reply(v)) => send_js(&webview, "reply", v),
            Event::UserEvent(UserEvent::Title(t)) => window.set_title(&t),
            Event::UserEvent(UserEvent::Watch(dir)) => {
                if let Some(w) = watcher.as_mut() {
                    if watched.as_ref() != Some(&dir) {
                        if let Some(old) = watched.take() {
                            let _ = w.unwatch(&old);
                        }
                        if w.watch(&dir, RecursiveMode::NonRecursive).is_ok() {
                            watched = Some(dir);
                        }
                    }
                }
            }
            Event::UserEvent(UserEvent::FsChanged(paths)) => {
                let paths: Vec<_> = paths.iter().map(|p| p.to_string_lossy()).collect();
                send_js(&webview, "fsChanged", json!(paths));
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
            let _ = proxy.send_event(UserEvent::Watch(PathBuf::from(arg)));
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
            let result = match server::save_file(Path::new(path), arg) {
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
