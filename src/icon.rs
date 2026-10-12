//! The app icon: on the window, in the Dock (macOS), and in a desktop entry
//! (Linux, where Wayland panels only show icons from an installed .desktop file).

use tao::window::Icon;

/// Window icon (taskbar and title bar on Windows and X11).
pub fn window_icon() -> Option<Icon> {
    let decoder = png::Decoder::new(std::io::Cursor::new(
        &include_bytes!("../icons/mdview-128.png")[..],
    ));
    let mut reader = decoder.read_info().ok()?;
    let mut rgba = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut rgba).ok()?;
    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        return None;
    }
    rgba.truncate(info.buffer_size());
    Icon::from_rgba(rgba, info.width, info.height).ok()
}

/// Makes the icon show up where the OS looks for it. Call after the event loop exists.
pub fn install() {
    #[cfg(target_os = "macos")]
    dock_icon();
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    desktop_entry();
}

#[cfg(target_os = "macos")]
fn dock_icon() {
    use objc2::{AllocAnyThread, MainThreadMarker};
    use objc2_app_kit::{NSApplication, NSImage};
    use objc2_foundation::NSData;
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let data = NSData::with_bytes(include_bytes!("../icons/mdview-512.png"));
    let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) else {
        return;
    };
    // SAFETY: called on the main thread with a valid image.
    unsafe { NSApplication::sharedApplication(mtm).setApplicationIconImage(Some(&image)) };
}

/// Writes `mdview.desktop` and the icon into the user's data folder (once, and
/// again if the binary moves). GTK uses the program name as the Wayland app id,
/// so the entry's name is what links the window to its icon. Set
/// MDVIEW_NO_DESKTOP_ENTRY=1 to skip this.
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn desktop_entry() {
    use std::fs;
    if std::env::var_os("MDVIEW_NO_DESKTOP_ENTRY").is_some() {
        return;
    }
    let (Some(data), Ok(exe)) = (dirs::data_dir(), std::env::current_exe()) else {
        return;
    };
    // Desktop-entry Exec quoting: wrap in quotes, escape " ` $ \.
    let mut quoted = String::from("\"");
    for c in exe.to_string_lossy().chars() {
        if matches!(c, '"' | '`' | '$' | '\\') {
            quoted.push('\\');
        }
        quoted.push(c);
    }
    quoted.push('"');
    let entry = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=mdview\n\
         GenericName=Markdown Viewer\n\
         Comment=View and edit Markdown files\n\
         Exec={quoted} %f\n\
         Icon=mdview\n\
         Terminal=false\n\
         Categories=Utility;TextEditor;\n\
         MimeType=text/markdown;text/x-markdown;\n\
         StartupWMClass=mdview\n"
    );
    let files: [(&str, &[u8]); 3] = [
        (
            "icons/hicolor/scalable/apps/mdview.svg",
            include_bytes!("../icons/mdview.svg"),
        ),
        (
            "icons/hicolor/256x256/apps/mdview.png",
            include_bytes!("../icons/mdview-256.png"),
        ),
        ("applications/mdview.desktop", entry.as_bytes()),
    ];
    for (rel, bytes) in files {
        let path = data.join(rel);
        if fs::read(&path).is_ok_and(|old| old == bytes) {
            continue;
        }
        let _ = path.parent().map(fs::create_dir_all);
        let _ = fs::write(&path, bytes);
    }
}
