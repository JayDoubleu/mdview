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

pub const MARKER: &str = "<!-- ANNOTATION";

/// Replaces the annotation comments in rendered `html` with note cards.
/// `src` is the Markdown it came from, used to give each card its source lines.
pub fn render(html: &str, src: &str) -> String {
    if !html.contains(MARKER) {
        return html.to_string();
    }
    let lines = source_lines(src);
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    let mut n = 0;
    while let Some(i) = rest.find(MARKER) {
        let body_start = i + MARKER.len();
        let Some(len) = rest[body_start..].find("-->") else {
            break;
        };
        out.push_str(&rest[..i]);
        // The positions are only trusted when every comment was matched.
        let pos = lines.get(n).filter(|_| lines.len() == count(html));
        out.push_str(&card(&rest[body_start..body_start + len], pos));
        rest = &rest[body_start + len + 3..];
        n += 1;
    }
    out.push_str(rest);
    out
}

fn count(html: &str) -> usize {
    html.matches(MARKER).count()
}

/// First and last line of each annotation comment in `src`, skipping code fences.
fn source_lines(src: &str) -> Vec<(usize, usize)> {
    let mut found = Vec::new();
    let mut fence: Option<&str> = None;
    let mut open: Option<usize> = None;
    for (i, line) in src.lines().enumerate() {
        let t = line.trim_start();
        if let Some(start) = open {
            if line.contains("-->") {
                found.push((start, i + 1));
                open = None;
            }
            continue;
        }
        if let Some(f) = fence {
            if t.starts_with(f) {
                fence = None;
            }
            continue;
        }
        if t.starts_with("```") {
            fence = Some("```");
        } else if t.starts_with("~~~") {
            fence = Some("~~~");
        } else if let Some(rest) = line.strip_prefix(MARKER) {
            if rest.contains("-->") {
                found.push((i + 1, i + 1));
            } else {
                open = Some(i + 1);
            }
        }
    }
    found
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

fn card(body: &str, pos: Option<&(usize, usize)>) -> String {
    let (quote, note) = parse(body);
    let pos = pos
        .map(|(a, b)| format!(" data-sourcepos=\"{a}:1-{b}:3\""))
        .unwrap_or_default();
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
}
