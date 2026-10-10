// Hide the console window for release builds on Windows.
#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

mod render;
mod server;

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
    let _menu = macos_menu();

    let builder = WebViewBuilder::new_with_web_context(&mut web_context)
        .with_custom_protocol("mdview".into(), move |_id, req| {
            server::handle(&proto_launch, req)
        })
        .with_url(format!("{origin}/"))
        .with_devtools(cfg!(debug_assertions))
        .with_ipc_handler(move |req| handle_ipc(&ipc_proxy, req.body()))
        .with_navigation_handler(move |url| {
            if url.starts_with(origin) || url.starts_with("about:") {
                true
            } else {
                let _ = open::that_detached(&url);
                false
            }
        })
        .with_new_window_req_handler(|url, _| {
            let _ = open::that_detached(&url);
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
        builder.build_gtk(window.default_vbox().expect("no gtk vbox"))?
    };

    let mut watcher = make_watcher(proxy.clone());
    let mut watched: Option<PathBuf> = None;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            }
            | Event::UserEvent(UserEvent::Quit) => {
                *control_flow = ControlFlow::Exit;
            }
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

/// macOS needs an application menu for standard shortcuts (Cmd+C, Cmd+A, Cmd+Q...) to work.
#[cfg(target_os = "macos")]
fn macos_menu() -> muda::Menu {
    use muda::{Menu, PredefinedMenuItem as P, Submenu};
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
            &P::quit(None),
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
        Some("open") => {
            let _ = open::that_detached(arg);
        }
        Some("quit") => {
            let _ = proxy.send_event(UserEvent::Quit);
        }
        _ => {}
    }
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
