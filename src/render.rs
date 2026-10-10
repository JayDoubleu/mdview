//! Markdown -> HTML rendering (GitHub flavoured) and syntax-highlighted source view.

use comrak::adapters::CodefenceRendererAdapter;
use comrak::nodes::Sourcepos;
use comrak::plugins::syntect::{SyntectAdapter, SyntectAdapterBuilder};
use comrak::{Options, markdown_to_html_with_plugins, options::Plugins};
use std::fmt;
use std::sync::OnceLock;
use syntect::highlighting::ThemeSet;
use syntect::html::{ClassStyle, ClassedHTMLGenerator, css_for_theme_with_class_style};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

/// Prefix for highlighter classes so they never collide with the app's own CSS.
const CLASS_PREFIX: &str = "hl-";
const CLASS_STYLE: ClassStyle = ClassStyle::SpacedPrefixed {
    prefix: CLASS_PREFIX,
};

struct Engine {
    adapter: SyntectAdapter,
    syntaxes: SyntaxSet,
}

fn engine() -> &'static Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE.get_or_init(|| Engine {
        adapter: SyntectAdapterBuilder::new()
            .css_with_class_prefix(CLASS_PREFIX)
            .build(),
        syntaxes: SyntaxSet::load_defaults_newlines(),
    })
}

fn options() -> Options<'static> {
    let mut o = Options::default();
    let e = &mut o.extension;
    e.strikethrough = true;
    e.table = true;
    e.autolink = true;
    e.tasklist = true;
    e.footnotes = true;
    e.header_id_prefix = Some(String::new());
    e.front_matter_delimiter = Some("---".into());
    e.alerts = true;
    e.math_dollars = true;
    e.math_code = true;
    e.description_lists = true;
    e.superscript = false;
    e.shortcodes = true;
    o.parse.relaxed_autolinks = true;
    o.render.r#unsafe = true;
    o.render.github_pre_lang = true;
    o.render.sourcepos = true;
    o.render.tasklist_classes = true;
    o
}

/// Leaves mermaid diagrams as escaped source for the frontend to render.
struct Mermaid;

impl CodefenceRendererAdapter for Mermaid {
    fn write(
        &self,
        out: &mut dyn fmt::Write,
        _lang: &str,
        _meta: &str,
        code: &str,
        sourcepos: Option<Sourcepos>,
    ) -> fmt::Result {
        let pos = sourcepos
            .map(|p| format!(" data-sourcepos=\"{p}\""))
            .unwrap_or_default();
        writeln!(
            out,
            "<pre class=\"mermaid-src\"{pos}><code>{}</code></pre>",
            html_escape(code)
        )
    }
}

/// Renders markdown to HTML using GitHub flavoured extensions.
pub fn markdown(src: &str) -> String {
    let opts = options();
    let mut plugins = Plugins::default();
    plugins.render.codefence_syntax_highlighter = Some(&engine().adapter);
    plugins
        .render
        .codefence_renderers
        .insert("mermaid".into(), &Mermaid);
    markdown_to_html_with_plugins(src, &opts, &plugins)
}

/// Highlights raw markdown source, returning HTML for the inside of a `<code>` element.
pub fn source(src: &str) -> String {
    let e = engine();
    let syntax = e
        .syntaxes
        .find_syntax_by_extension("md")
        .unwrap_or_else(|| e.syntaxes.find_syntax_plain_text());
    let mut generator =
        ClassedHTMLGenerator::new_with_class_style(syntax, &e.syntaxes, CLASS_STYLE);
    for line in LinesWithEndings::from(src) {
        if generator
            .parse_html_for_line_which_includes_newline(line)
            .is_err()
        {
            return html_escape(src);
        }
    }
    generator.finalize()
}

pub fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

/// CSS for syntax highlighting classes, with a light and a dark variant.
pub fn highlight_css() -> String {
    let themes = ThemeSet::load_defaults();
    let mut css = String::new();
    for (theme, scope) in [
        ("InspiredGitHub", ":root[data-theme=light]"),
        ("base16-ocean.dark", ":root[data-theme=dark]"),
    ] {
        let raw =
            css_for_theme_with_class_style(&themes.themes[theme], CLASS_STYLE).unwrap_or_default();
        css.push_str(&scope_css(&raw, scope));
    }
    css
}

/// Prefixes every selector in a flat stylesheet with `scope`.
fn scope_css(css: &str, scope: &str) -> String {
    let mut out = String::new();
    let mut rest = css;
    while let Some(open) = rest.find('{') {
        let Some(close) = rest[open..].find('}') else {
            break;
        };
        let selectors = strip_comments(&rest[..open]);
        let body = &rest[open..open + close + 1];
        let scoped: Vec<String> = selectors
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| format!("{scope} {s}"))
            .collect();
        // The `.code` rule sets the page background; the app styles that itself.
        if !scoped.is_empty() && selectors.trim() != ".hl-code" {
            out.push_str(&scoped.join(", "));
            out.push_str(body);
            out.push('\n');
        }
        rest = &rest[open + close + 1..];
    }
    out
}

fn strip_comments(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(start) = rest.find("/*") {
        out.push_str(&rest[..start]);
        match rest[start..].find("*/") {
            Some(end) => rest = &rest[start + end + 2..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_gfm() {
        let html = markdown("# Hi\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n- [x] done\n\n~~gone~~\n");
        assert!(html.contains("<table"));
        assert!(html.contains("checkbox"));
        assert!(html.contains("<del"));
        assert!(html.contains("id=\"hi\""));
    }

    #[test]
    fn highlights_code_fences() {
        let html = markdown("```rust\nfn main() {}\n```\n");
        assert!(html.contains("class=\""), "{html}");
        assert!(html.contains("hl-source hl-rust"), "{html}");
    }

    #[test]
    fn mermaid_fences_are_preserved() {
        let html = markdown("```mermaid\ngraph TD; A-->B\n```\n");
        assert!(html.contains("class=\"mermaid-src\""), "{html}");
        assert!(html.contains("A--&gt;B"), "{html}");
    }

    #[test]
    fn alerts() {
        let html = markdown("> [!NOTE]\n> hello\n");
        assert!(html.contains("markdown-alert"), "{html}");
    }

    #[test]
    fn scopes_css() {
        let css = scope_css(
            "/* c */\n.a, .b {\n color: red;\n}\n.hl-code { x: y; }",
            ":root",
        );
        assert_eq!(css, ":root .a, :root .b{\n color: red;\n}\n");
        assert!(!highlight_css().is_empty());
    }
}
