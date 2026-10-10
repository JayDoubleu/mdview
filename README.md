# mdview

A small, fast Markdown viewer for macOS, Windows and Linux. You open it from the terminal the same way you'd use `code`:

```sh
mdview README.md      # open a file
mdview docs/          # open a folder (shows its README.md if there is one)
mdview                # browse the current folder
```

![mdview screenshot](docs/screenshot.png)

## Features

- **GitHub-flavoured Markdown.** Tables, task lists, strikethrough, autolinks, footnotes, alerts (`> [!NOTE]`), emoji shortcodes (`:tada:`), heading anchors and raw HTML.
- **Syntax highlighting** for fenced code blocks. Each block gets a copy button.
- **Mermaid diagrams** and **math** (`$inline$`, `$$display$$` and ` ```math ` blocks) via bundled Mermaid and KaTeX. Both work offline.
- **File tree** on the left. It opens at the file's folder. Click a folder to expand it, double-click it (or use a breadcrumb) to make it the root, and use ↑ or <kbd>Backspace</kbd> to go up a level. The 👁 button shows non-Markdown files too.
- **Source pane** on the right (<kbd>Ctrl/Cmd+E</kbd>). It shows the raw Markdown with line numbers and highlighting, and scrolls in sync with the preview.
- **Live reload.** When you save the file in your editor, the preview refreshes and keeps your scroll position.
- Light and dark themes (follows the OS; toggle with <kbd>Ctrl/Cmd+L</kbd>), zoom, find-in-page, back/forward history, and drag-and-drop to open a file.
- Relative links to other `.md` files open inside mdview. Web links open in your browser. Local images load as you'd expect.
- About 9 MB as a single binary. It uses the system webview (WebView2, WKWebView or WebKitGTK), so nothing like Electron gets bundled.

## Install

### Prebuilt binaries

Every push builds binaries for all three platforms in GitHub Actions. Download `mdview-<platform>` from the [Actions tab](https://github.com/JayDoubleu/mdview/actions) (or from [Releases](https://github.com/JayDoubleu/mdview/releases) once a `v*` tag is pushed), extract it, and put `mdview` somewhere on your `PATH`.

- **macOS:** the binary isn't signed, so clear the quarantine flag once: `xattr -d com.apple.quarantine ./mdview`.
- **Windows:** needs the WebView2 runtime, which ships with Windows 10 and 11.
- **Linux:** needs WebKitGTK 4.1 (`sudo apt install libwebkit2gtk-4.1-0`, `sudo dnf install webkit2gtk4.1`, or `sudo pacman -S webkit2gtk-4.1`).

### From source

You need [Rust](https://rustup.rs) 1.85 or newer.

```sh
# Linux only: build dependencies
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev      # Debian / Ubuntu
sudo dnf install webkit2gtk4.1-devel gtk3-devel          # Fedora
sudo pacman -S webkit2gtk-4.1 gtk3                       # Arch

cargo install --git https://github.com/JayDoubleu/mdview
```

Or clone the repo and run `cargo install --path .`. Either way the binary lands in `~/.cargo/bin`.

## Usage

```
mdview [PATH]

  -w, --wait       Keep the terminal attached until the window is closed
  -h, --help       Print help
  -V, --version    Print version
```

By default `mdview` hands the terminal back straight away, like `code`. Pass `--wait` if you want it to block.

| Shortcut (Cmd on macOS) | Action |
|---|---|
| <kbd>Ctrl+B</kbd> | Toggle file tree |
| <kbd>Ctrl+E</kbd> | Toggle source pane |
| <kbd>Ctrl+F</kbd> | Find in document |
| <kbd>Ctrl+L</kbd> | Toggle light / dark |
| <kbd>Ctrl+R</kbd> | Reload |
| <kbd>Ctrl+=</kbd> / <kbd>Ctrl+-</kbd> / <kbd>Ctrl+0</kbd> | Zoom in / out / reset |
| <kbd>Alt+←</kbd> / <kbd>Alt+→</kbd> | Back / forward |
| <kbd>Backspace</kbd> | Tree: go up one folder |
| <kbd>Ctrl+W</kbd> / <kbd>Ctrl+Q</kbd> | Close |

You can drag the dividers to resize the panes. mdview remembers pane sizes, theme and zoom between runs.

## How it works

- `src/main.rs` creates the window ([tao](https://github.com/tauri-apps/tao)) and the webview ([wry](https://github.com/tauri-apps/wry)), watches the open file's folder ([notify](https://github.com/notify-rs/notify)), and handles the CLI.
- `src/server.rs` serves a `mdview://` custom protocol. It provides the embedded UI assets plus a small JSON API for listing folders, rendering files and loading local images.
- `src/render.rs` renders Markdown with [comrak](https://github.com/kivikakk/comrak) (GFM extensions) and highlights code with [syntect](https://github.com/trishume/syntect).
- `assets/` holds the UI (plain HTML, CSS and JS, no build step) and the vendored Mermaid and KaTeX. `build.rs` embeds all of it into the binary.

## License

MIT. Bundled third-party code: Mermaid (MIT) and KaTeX (MIT). Their license files are in `assets/vendor/`.
