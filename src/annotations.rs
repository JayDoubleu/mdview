//! Annotations: notes left on a selection for a person or an AI agent to act on.
//!
//! They live in the Markdown as HTML comments, so other renderers (GitHub,
//! editors) hide them while anything reading the file sees them plainly:
//!
//! ```text
//! <!-- ANNOTATION
//! Selected: "the selected Markdown"
//! Note: what should change
//! -->
//! ```
//!
//! A selection spanning several lines is quoted with `> ` lines instead. The
//! preview shows each annotation as a note card.

use crate::render::html_escape;
use comrak::nodes::{AstNode, NodeValue};

pub const MARKER: &str = "<!-- ANNOTATION";

/// Turns annotation comments in the parsed document into note cards.
///
/// Only HTML blocks that start with the marker count: comrak has already
/// decided they aren't code, and gives their exact source position. Inline
/// comments and anything else stay ordinary (hidden) comments.
pub fn mark_cards<'a>(root: &'a AstNode<'a>) {
    for node in root.descendants() {
        let mut data = node.data.borrow_mut();
        let pos = data.sourcepos;
        let NodeValue::HtmlBlock(block) = &mut data.value else {
            continue;
        };
        let Some(body) = block.literal.trim_start().strip_prefix(MARKER) else {
            continue;
        };
        let Some(end) = body.find("-->") else {
            continue;
        };
        let range = (pos.start.line, pos.start.column, pos.end.line);
        let card = card(&body[..end], range);
        // Anything after the comment on its last line is left as it was.
        block.literal = card + &body[end + 3..];
    }
}

/// Splits a comment body into the quoted selection and the note.
fn parse(body: &str) -> (String, String) {
    let mut quote = Vec::new();
    let mut note = Vec::new();
    let mut in_note = false;
    for line in body.lines() {
        if let Some(r) = line.strip_prefix("Selected:") {
            let r = r.trim();
            if !r.is_empty() {
                let r = r
                    .strip_prefix('"')
                    .and_then(|r| r.strip_suffix('"'))
                    .unwrap_or(r);
                quote.push(r);
            }
            in_note = false;
        } else if let Some(r) = line.strip_prefix("Note:") {
            note.push(r.trim_start());
            in_note = true;
        } else if in_note {
            note.push(line);
        } else if let Some(q) = line.strip_prefix('>') {
            quote.push(q.strip_prefix(' ').unwrap_or(q));
        } else if !line.trim().is_empty() {
            // Not in the usual shape: show it all as the note.
            note.push(line);
            in_note = true;
        }
    }
    (quote.join("\n"), note.join("\n").trim().to_string())
}

/// `pos` is the comment's first line and column and its last line.
fn card(body: &str, pos: (usize, usize, usize)) -> String {
    let (quote, note) = parse(body);
    let (a, col, b) = pos;
    let pos = format!(" data-sourcepos=\"{a}:{col}-{b}:3\"");
    let mut out = format!("<aside class=\"annotation\"{pos}><div class=\"annotation-note\">");
    out.push_str(&html_escape(if note.is_empty() {
        "(empty note)"
    } else {
        &note
    }));
    out.push_str("</div>");
    if !quote.is_empty() {
        out.push_str("<div class=\"annotation-quote\">");
        out.push_str(&html_escape(&quote));
        out.push_str("</div>");
    }
    out.push_str("</aside>\n");
    out
}

#[cfg(test)]
mod tests {
    use crate::render::markdown;

    #[test]
    fn renders_cards_with_source_lines() {
        let src = "# T\n\n<!-- ANNOTATION\nSelected: \"some **text**\"\nNote: Make it <shorter>\n-->\n\nsome **text**\n\n```\n<!-- ANNOTATION\n```\n";
        let html = markdown(src);
        assert!(
            html.contains("<aside class=\"annotation\" data-sourcepos=\"3:1-6:3\">"),
            "{html}"
        );
        assert!(html.contains("Make it &lt;shorter&gt;"), "{html}");
        assert!(html.contains("some **text**</div>"), "{html}");
        assert!(
            html.contains("&lt;!-- ANNOTATION"),
            "code stays code: {html}"
        );
        // The paragraph after it keeps its line numbers.
        assert!(html.contains("<p data-sourcepos=\"8:1-8:13\">"), "{html}");
    }

    #[test]
    fn multi_line_quotes_and_notes() {
        let src = "<!-- ANNOTATION\nSelected:\n> line one\n> line two\nNote: first\nsecond\n-->\n\ntext\n";
        let html = markdown(src);
        assert!(html.contains("line one\nline two</div>"), "{html}");
        assert!(html.contains("first\nsecond</div>"), "{html}");
    }

    #[test]
    fn plain_comments_are_still_hidden() {
        let html = markdown("<!-- just a comment -->\n\ntext\n");
        assert!(!html.contains("annotation"), "{html}");
        assert!(!html.contains("just a comment"), "{html}");
    }

    #[test]
    fn positions_come_from_the_parser() {
        // A ```` fence closed only by ````, holding a ``` line and a marker,
        // followed by a real note inside a blockquote.
        let src = "````md\n```\n<!-- ANNOTATION\nNote: ex\n-->\n````\n\n> <!-- ANNOTATION\n> Note: real\n> -->\n";
        let html = markdown(src);
        assert_eq!(html.matches("<aside").count(), 1, "{html}");
        assert!(html.contains("data-sourcepos=\"8:3-10:3\""), "{html}");
        // Inside a list item, and inline in a paragraph (stays hidden).
        let html = markdown(
            "- item\n\n  <!-- ANNOTATION\n  Note: n\n  -->\n\ntext <!-- ANNOTATION Note: x --> more\n",
        );
        assert!(html.contains("data-sourcepos=\"3:3-5:3\""), "{html}");
        assert_eq!(html.matches("<aside").count(), 1, "{html}");
    }

    #[test]
    fn many_notes_render_quickly() {
        let src = "<!-- ANNOTATION\nNote: n\n-->\n\n".repeat(20_000);
        let t = std::time::Instant::now();
        let html = markdown(&src);
        assert_eq!(html.matches("<aside").count(), 20_000);
        assert!(t.elapsed().as_secs() < 10, "took {:?}", t.elapsed());
    }

    #[test]
    fn notes_cannot_inject_markup() {
        let html = markdown(
            "<!-- ANNOTATION\nSelected: \"<img src=x onerror=alert(1)>\"\nNote: <script>alert(1)</script>\" onmouseover=\"x\n-->\n",
        );
        assert!(!html.contains("<script"), "{html}");
        assert!(!html.contains("<img"), "{html}");
        // The quote can't escape its element: still exactly the two divs.
        assert_eq!(html.matches('<').count(), 6, "{html}");
    }
}
