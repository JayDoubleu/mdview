//! Markdown -> HTML rendering (GitHub flavoured) and syntax-highlighted source view.

use comrak::adapters::{CodefenceRendererAdapter, SyntaxHighlighterAdapter};
use comrak::nodes::Sourcepos;
use comrak::plugins::syntect::{SyntectAdapter, SyntectAdapterBuilder};
use comrak::{Options, markdown_to_html_with_plugins, options::Plugins};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
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
    adapter: Highlighter,
    syntaxes: SyntaxSet,
}

fn engine() -> &'static Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE.get_or_init(|| Engine {
        adapter: Highlighter(
            SyntectAdapterBuilder::new()
                .css_with_class_prefix(CLASS_PREFIX)
                .build(),
        ),
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

/// Syntect highlighting that keeps the `<pre>`'s `data-sourcepos` and `lang`
/// (the stock adapter drops them), so code blocks map back to source lines.
struct Highlighter(SyntectAdapter);

impl SyntaxHighlighterAdapter for Highlighter {
    fn write_highlighted(
        &self,
        output: &mut dyn fmt::Write,
        lang: Option<&str>,
        code: &str,
    ) -> fmt::Result {
        self.0.write_highlighted(output, lang, code)
    }

    fn write_pre_tag(
        &self,
        output: &mut dyn fmt::Write,
        mut attributes: HashMap<&'static str, Cow<'_, str>>,
    ) -> fmt::Result {
        let mut attrs = vec![("class", Cow::Borrowed("syntax-highlighting"))];
        for key in ["lang", "data-sourcepos"] {
            if let Some(v) = attributes.remove(key) {
                attrs.push((key, v));
            }
        }
        comrak::html::write_opening_tag(output, "pre", attrs)
    }

    fn write_code_tag(
        &self,
        output: &mut dyn fmt::Write,
        attributes: HashMap<&'static str, Cow<'_, str>>,
    ) -> fmt::Result {
        self.0.write_code_tag(output, attributes)
    }
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
///
/// Raw HTML is allowed (as on GitHub) but sanitized: documents may come from
/// untrusted repositories, and the page can read local files through the app.
pub fn markdown(src: &str) -> String {
    let opts = options();
    let mut plugins = Plugins::default();
    plugins.render.codefence_syntax_highlighter = Some(&engine().adapter);
    plugins
        .render
        .codefence_renderers
        .insert("mermaid".into(), &Mermaid);
    // comrak skips YAML front matter (keeping line numbers intact); show it as
    // a collapsible panel at the top instead of dropping it silently. TOML
    // front matter (Hugo's +++) gets the same treatment: its delimiters are
    // swapped for --- so comrak skips it too.
    let fm = front_matter(src);
    let toml_src;
    let body_src = match &fm {
        Some(f) if f.toml => {
            toml_src = src
                .split_inclusive('\n')
                .enumerate()
                .map(|(i, l)| {
                    if i == 0 || i + 1 == f.last_line {
                        "---\n"
                    } else {
                        l
                    }
                })
                .collect::<String>();
            toml_src.as_str()
        }
        _ => src,
    };
    let mut html = fm.map(front_matter_html).unwrap_or_default();
    html.push_str(&markdown_to_html_with_plugins(body_src, &opts, &plugins));
    let html = crate::annotations::render(&html, body_src);
    sanitizer().clean(&html).to_string()
}

struct FrontMatter<'a> {
    text: &'a str,
    last_line: usize,
    toml: bool,
}

/// A leading `---` YAML (closed by `---` or `...`) or `+++` TOML block.
fn front_matter(src: &str) -> Option<FrontMatter<'_>> {
    let mut lines = src.split_inclusive('\n');
    let toml = match lines.next()?.trim_end() {
        "---" => false,
        "+++" => true,
        _ => return None,
    };
    let start = src.find('\n')? + 1;
    let mut offset = start;
    for (i, line) in lines.enumerate() {
        let end = line.trim_end();
        if (toml && end == "+++") || (!toml && matches!(end, "---" | "...")) {
            return Some(FrontMatter {
                text: &src[start..offset],
                last_line: i + 2,
                toml,
            });
        }
        offset += line.len();
    }
    None
}

/// Top-level YAML keys: unindented `key:` lines.
fn yaml_fields(yaml: &str) -> usize {
    yaml.lines()
        .filter(|l| {
            l.chars()
                .next()
                .is_some_and(|c| !c.is_whitespace() && !"#-".contains(c))
        })
        .filter(|l| l.contains(':'))
        .count()
}

/// Top-level TOML keys: `key =` lines before the first table, plus each table.
fn toml_fields(toml: &str) -> usize {
    let mut tables = std::collections::BTreeSet::new();
    let mut keys = 0;
    for line in toml.lines().map(str::trim) {
        if let Some(name) = line.strip_prefix('[') {
            tables.insert(
                name.trim_matches(['[', ']'])
                    .split('.')
                    .next()
                    .unwrap_or("")
                    .trim(),
            );
        } else if tables.is_empty() && !line.starts_with('#') && line.contains('=') {
            keys += 1;
        }
    }
    keys + tables.len()
}

fn front_matter_html(fm: FrontMatter) -> String {
    let FrontMatter {
        text: yaml,
        last_line,
        toml,
    } = fm;
    let fields = if toml {
        toml_fields(yaml)
    } else {
        yaml_fields(yaml)
    };
    let e = engine();
    let syntax = e
        .syntaxes
        .find_syntax_by_extension(if toml { "toml" } else { "yaml" })
        .unwrap_or_else(|| e.syntaxes.find_syntax_plain_text());
    let mut generator =
        ClassedHTMLGenerator::new_with_class_style(syntax, &e.syntaxes, CLASS_STYLE);
    let mut ok = true;
    for line in LinesWithEndings::from(yaml) {
        ok &= generator
            .parse_html_for_line_which_includes_newline(line)
            .is_ok();
    }
    let code = if ok {
        generator.finalize()
    } else {
        html_escape(yaml)
    };
    let plural = if fields == 1 { "" } else { "s" };
    format!(
        "<details class=\"front-matter\" data-sourcepos=\"1:1-{last_line}:3\">\
         <summary>Front matter <span class=\"fm-count\">{fields} field{plural}</span></summary>\
         <pre class=\"syntax-highlighting\"><code>{code}</code></pre></details>\n"
    )
}

fn sanitizer() -> &'static ammonia::Builder<'static> {
    static SANITIZER: OnceLock<ammonia::Builder<'static>> = OnceLock::new();
    SANITIZER.get_or_init(|| {
        let mut b = ammonia::Builder::default();
        b.add_tags([
            "section", "aside", "input", "picture", "source", "video", "audio",
        ])
        .add_generic_attributes(["class", "id", "align", "dir", "aria-label", "aria-hidden"])
        .generic_attribute_prefixes(HashSet::from(["data-"]))
        .add_tag_attributes("a", ["name"])
        .add_tag_attributes("img", ["loading"])
        .add_tag_attributes("input", ["type", "checked", "disabled"])
        .add_tag_attributes("source", ["src", "srcset", "type", "media"])
        .add_tag_attributes(
            "video",
            ["src", "controls", "width", "height", "loop", "muted"],
        )
        .add_tag_attributes("audio", ["src", "controls", "loop"])
        .url_schemes(HashSet::from(["http", "https", "mailto"]))
        .link_rel(None);
        b
    })
}

/// Highlights raw markdown source, returning one `<span class="line">` per source line
/// so every row has the same height as the line-number gutter.
pub fn source(src: &str) -> String {
    split_lines(&highlight_source(src))
}

fn highlight_source(src: &str) -> String {
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

/// Wraps each line of highlighted HTML in its own element. Highlight spans can
/// cross line breaks, so open spans are closed at the end of a line and
/// reopened at the start of the next one.
fn split_lines(html: &str) -> String {
    let mut out = String::with_capacity(html.len() * 2);
    let mut open: Vec<&str> = Vec::new();
    let mut rest = html;
    out.push_str("<span class=\"line\">");
    while let Some(i) = rest.find(['<', '\n']) {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        if rest.starts_with('\n') {
            out.push_str(&"</span>".repeat(open.len()));
            out.push_str("\n</span><span class=\"line\">");
            open.iter().for_each(|tag| out.push_str(tag));
            rest = &rest[1..];
        } else {
            let end = rest.find('>').map_or(rest.len(), |e| e + 1);
            let tag = &rest[..end];
            if tag.starts_with("</") {
                open.pop();
            } else {
                open.push(tag);
            }
            out.push_str(tag);
            rest = &rest[end..];
        }
    }
    out.push_str(rest);
    out.push_str(&"</span>".repeat(open.len()));
    out.push_str("</span>");
    out
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
        // The preview maps code blocks back to their source lines.
        assert!(html.contains("data-sourcepos=\"1:1-3:3\""), "{html}");
    }

    #[test]
    fn mermaid_fences_are_preserved() {
        let html = markdown("```mermaid\ngraph TD; A-->B\n```\n");
        assert!(html.contains("class=\"mermaid-src\""), "{html}");
        assert!(html.contains("A--&gt;B"), "{html}");
    }

    #[test]
    fn source_lines_are_balanced() {
        let html = split_lines("<span class=\"a\">x\ny</span>\nz");
        assert_eq!(
            html,
            "<span class=\"line\"><span class=\"a\">x</span>\n</span>\
             <span class=\"line\"><span class=\"a\">y</span>\n</span>\
             <span class=\"line\">z</span>"
        );
        let src = "# a\n\n```rust\nfn x() {}\n```\n";
        assert_eq!(
            source(src).matches("class=\"line\"").count(),
            src.lines().count() + 1
        );
    }

    #[test]
    fn sanitizes_raw_html() {
        let html = markdown(
            "<script>alert(1)</script>\n\n<iframe src=\"x.html\"></iframe>\n\n\
             <img src=\"a.png\" onerror=\"alert(1)\">\n\n[x](javascript:alert(1))\n\n\
             <details><summary>s</summary>\n\nok\n\n</details>\n",
        );
        for bad in ["<script", "alert", "<iframe", "onerror", "javascript:"] {
            assert!(!html.contains(bad), "{bad} in {html}");
        }
        assert!(
            html.contains("<details>") && html.contains("<img src=\"a.png\""),
            "{html}"
        );
    }

    #[test]
    fn sanitizer_keeps_gfm_markup() {
        let html = markdown("# T\n\n- [x] a\n\n$x$\n\nf[^1]\n\n[^1]: n\n\n> [!NOTE]\n> n\n");
        for good in [
            "id=\"t\"",
            "class=\"anchor\"",
            "type=\"checkbox\"",
            "checked",
            "data-math-style",
            "data-sourcepos",
            "<section",
            "markdown-alert-note",
        ] {
            assert!(html.contains(good), "{good} missing in {html}");
        }
    }

    #[test]
    fn front_matter_is_shown_collapsed() {
        let src = "---\ntitle: Hi\ntags:\n  - a\n---\n\n# Body\n";
        let html = markdown(src);
        assert!(
            html.starts_with("<details class=\"front-matter\" data-sourcepos=\"1:1-5:3\">"),
            "{html}"
        );
        assert!(html.contains("2 fields"), "{html}");
        assert!(html.contains("title"), "{html}");
        assert!(
            html.contains("data-sourcepos=\"7:1-7:6\""),
            "body keeps its line numbers: {html}"
        );
        assert!(!markdown("# No front matter\n").contains("class=\"front-matter\""));
        assert!(!markdown("---\nnot closed\n").contains("class=\"front-matter\""));
    }

    #[test]
    fn toml_front_matter() {
        let html = markdown("+++\ntitle = \"Hi\"\n[extra]\nx = 1\n+++\n\n# Body\n");
        assert!(html.contains("data-sourcepos=\"1:1-5:3\""), "{html}");
        assert!(html.contains("2 fields<"), "{html}");
        assert!(html.contains("data-sourcepos=\"7:1-7:6\""), "{html}");
        assert!(!html.contains("+++"), "{html}");
    }

    #[test]
    fn bom_and_empty() {
        assert!(markdown("").is_empty());
        assert!(markdown("\u{feff}# T\n").contains("<h1"));
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
