# Changelog

## Unreleased

- Basic editing: an Edit button unlocks the file in a plain text editor with a live preview. Changes are only written on Save (Ctrl/Cmd+S), unsaved state is shown in the toolbar, over the preview and in the window title, and locking, switching files or closing with unsaved changes asks before discarding them. Line endings (LF or CRLF) are preserved on save.
- Editor polish: line numbers, a highlighted cursor line (or selected lines), and a Discard button with Undo.
- The preview highlights the blocks for the cursor line or selection, in the editor and in the read-only source pane.
- Selection works both ways: selecting text in the preview selects the matching Markdown in the editor, or highlights it in the source pane.
- Highlighted code blocks keep their source position, so they take part in scroll sync and cursor highlighting.
- YAML front matter is shown as a collapsible panel at the top of the preview (it used to be hidden), and opens when the cursor or selection is inside it.
- TOML front matter (`+++`, Hugo) is shown in the same panel instead of as stray text.
- Site-root links and images such as `/uploads/a.png` resolve against the site folder (or its `static/` / `public/` folder), as static site generators do.
- Files in Windows-1252, UTF-16 or UTF-8 with a BOM display correctly and are saved back in the same encoding (falling back to UTF-8, with a notice, only if the text no longer fits).
- Empty files show a short note instead of a blank page; broken Mermaid diagrams show the parser error; slow loads show a "Rendering…" indicator.
- File tree: each folder shows how many Markdown files it contains, including subfolders.
- Annotations: right-click a selection (or press Ctrl/Cmd+Alt+M) to attach a note. It is stored in the file as an `<!-- ANNOTATION -->` comment that AI agents can read and act on, and the preview shows it as a card with a Resolve button.
- Live reload now covers every folder shown in the file tree, not only the open file's folder.
- Right-clicking no longer shows the webview's Back/Forward/Reload menu.

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
