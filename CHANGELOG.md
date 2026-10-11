# Changelog

## Unreleased

- Basic editing: an Edit button unlocks the file in a plain text editor with a live preview. Changes are only written on Save (Ctrl/Cmd+S), unsaved state is shown in the toolbar, over the preview and in the window title, and locking, switching files or closing with unsaved changes asks before discarding them. Line endings (LF or CRLF) are preserved on save.
- Editor polish: line numbers, a highlighted cursor line (or selected lines), and a Discard button with Undo.
- The preview highlights the blocks for the cursor line or selection, in the editor and in the read-only source pane.
- YAML front matter is shown as a collapsible panel at the top of the preview (it used to be hidden), and opens when the cursor or selection is inside it.
- File tree: each folder shows how many Markdown files it contains, including subfolders.

## 0.1.0

First release.

- GitHub-flavoured Markdown: tables, task lists, strikethrough, autolinks, footnotes, alerts, emoji shortcodes, heading anchors and sanitized raw HTML.
- Syntax-highlighted code blocks with copy buttons, Mermaid diagrams and KaTeX math, all bundled for offline use.
- File tree rooted at the opened file's folder, with navigation up and down the directory tree.
- Source pane with line numbers that scrolls in sync with the preview.
- Live reload on save, light and dark themes, zoom, find, back/forward history and drag-and-drop.
- `mdview` returns the terminal immediately, like `code`; `--wait` keeps it attached.
- Prebuilt binaries for Linux (x86_64, ARM64), macOS (universal) and Windows (x86_64, ARM64).
- Linux: instant wheel scrolling instead of WebKitGTK's animated scrolling, and a workaround for WebKitGTK stutter on NVIDIA drivers.
