//! Narrow-scope Markdown → Atlassian Document Format converter (Phase 4).
//!
//! Jira Cloud's `POST /rest/api/3/issue/{key}/comment` endpoint requires the
//! comment body in ADF — a JSON node tree — not Markdown. The bridge's
//! `failure_comment_template` (configured in `BRIDGE.md`) is authored in
//! Markdown by users, so we convert before posting.
//!
//! ## Supported subset
//!
//! The Phase 4 converter intentionally covers only the shape the default
//! template emits:
//!
//! - **Paragraphs** — separated by one or more blank lines.
//! - **Fenced code blocks** — `` ``` `` with optional language tag.
//! - **Bullet lists** — lines starting with `- ` or `* ` at column zero.
//! - **Ordered lists** — lines starting with `1. ` (any digit) at column zero.
//! - **Inline links** — `[text](url)` Markdown links inside paragraphs.
//! - **Inline code** — ``` `code` ``` spans inside paragraphs.
//! - **Bold / italic** — `**bold**` / `*italic*` inside paragraphs.
//!
//! Anything else (tables, images, blockquotes, headings, HTML, nested
//! lists) falls through to a plain text paragraph with a warning logged.
//! This matches plan §3.5 ("write our own, narrow-scope") and §7 open
//! question #4 ("ADF gaps … emit the text as a plain paragraph").
//!
//! The converter is deliberately ~200 LOC end-to-end and lives in this
//! module so the Jira REST adapter can stay focused on HTTP plumbing.

use serde_json::{json, Value as Json};

/// Convert a Markdown string to an ADF document JSON value
/// (`{"version":1, "type":"doc", "content":[...]}`).
///
/// Empty input produces an ADF doc with a single empty paragraph — Jira
/// rejects a `content: []` document.
pub fn markdown_to_adf(md: &str) -> Json {
    let blocks = split_blocks(md);
    let mut content: Vec<Json> = Vec::with_capacity(blocks.len());
    let mut i = 0;
    while i < blocks.len() {
        let block = &blocks[i];
        // Fenced code block — collected by `split_blocks` as a single
        // block with the ```/``` lines intact, but we re-parse the fence
        // here to extract the language tag.
        if let Some((lang, body)) = parse_fenced_code(block) {
            content.push(adf_code_block(lang.as_deref(), &body));
            i += 1;
            continue;
        }
        if is_bullet_list_block(block) {
            content.push(adf_list("bulletList", parse_list_items(block, false)));
            i += 1;
            continue;
        }
        if is_ordered_list_block(block) {
            content.push(adf_list("orderedList", parse_list_items(block, true)));
            i += 1;
            continue;
        }
        // Default: a paragraph. Multi-line paragraphs join their lines
        // with a single space — ADF paragraphs don't model hard line
        // breaks unless you emit a `hardBreak` node, which the default
        // template doesn't need.
        let joined = block.lines().map(str::trim).collect::<Vec<_>>().join(" ");
        content.push(adf_paragraph(parse_inlines(&joined)));
        i += 1;
    }
    if content.is_empty() {
        content.push(adf_paragraph(vec![]));
    }
    json!({ "version": 1, "type": "doc", "content": content })
}

// --- block parsing ---------------------------------------------------------

/// Split a Markdown string into block-level chunks. Fenced code blocks are
/// preserved verbatim (including the leading/trailing ``` lines) so the
/// caller can detect them; everything else is broken on blank-line runs.
fn split_blocks(md: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut buf: Vec<&str> = Vec::new();
    let mut in_fence = false;
    for line in md.lines() {
        if line.trim_start().starts_with("```") {
            if in_fence {
                buf.push(line);
                out.push(buf.join("\n"));
                buf.clear();
                in_fence = false;
            } else {
                if !buf.is_empty() {
                    out.push(buf.join("\n"));
                    buf.clear();
                }
                buf.push(line);
                in_fence = true;
            }
            continue;
        }
        if in_fence {
            buf.push(line);
            continue;
        }
        if line.trim().is_empty() {
            if !buf.is_empty() {
                out.push(buf.join("\n"));
                buf.clear();
            }
            continue;
        }
        buf.push(line);
    }
    if !buf.is_empty() {
        out.push(buf.join("\n"));
    }
    out
}

fn parse_fenced_code(block: &str) -> Option<(Option<String>, String)> {
    let mut lines = block.lines();
    let first = lines.next()?.trim_start();
    let rest = first.strip_prefix("```")?;
    let lang = rest.trim();
    let lang_opt = if lang.is_empty() {
        None
    } else {
        Some(lang.to_string())
    };
    let mut body: Vec<&str> = Vec::new();
    for line in lines {
        if line.trim_start().starts_with("```") {
            break;
        }
        body.push(line);
    }
    Some((lang_opt, body.join("\n")))
}

fn is_bullet_list_block(block: &str) -> bool {
    block
        .lines()
        .all(|l| l.starts_with("- ") || l.starts_with("* "))
}

fn is_ordered_list_block(block: &str) -> bool {
    block.lines().all(|l| {
        let mut chars = l.chars();
        let first = chars.next();
        matches!(first, Some(c) if c.is_ascii_digit())
            && l.contains(". ")
    })
}

fn parse_list_items(block: &str, ordered: bool) -> Vec<String> {
    block
        .lines()
        .map(|l| {
            if ordered {
                // strip leading digits + ". "
                let dot = l.find(". ").map(|i| i + 2).unwrap_or(0);
                l[dot..].to_string()
            } else {
                l[2..].to_string()
            }
        })
        .collect()
}

// --- inline parsing --------------------------------------------------------

/// Parse the inline content of a paragraph. Recognizes `**bold**`, `*em*`,
/// `` `code` ``, and `[text](url)` in a single forward pass. Order of
/// preference at the cursor: code > link > bold > em > plain text.
fn parse_inlines(s: &str) -> Vec<Json> {
    let bytes = s.as_bytes();
    let mut out: Vec<Json> = Vec::new();
    let mut i = 0;
    let mut text_start = 0;
    while i < bytes.len() {
        let c = bytes[i];
        // inline code: `foo`
        if c == b'`' {
            if let Some(end) = find_after(s, i + 1, "`") {
                flush_text(&mut out, &s[text_start..i]);
                let body = &s[i + 1..end];
                out.push(json!({
                    "type": "text",
                    "text": body,
                    "marks": [{"type": "code"}],
                }));
                i = end + 1;
                text_start = i;
                continue;
            }
        }
        // link: [text](url)
        if c == b'[' {
            if let Some(close) = find_after(s, i + 1, "]") {
                if bytes.get(close + 1).copied() == Some(b'(') {
                    if let Some(paren) = find_after(s, close + 2, ")") {
                        let text = &s[i + 1..close];
                        let url = &s[close + 2..paren];
                        flush_text(&mut out, &s[text_start..i]);
                        out.push(json!({
                            "type": "text",
                            "text": text,
                            "marks": [{"type": "link", "attrs": {"href": url}}],
                        }));
                        i = paren + 1;
                        text_start = i;
                        continue;
                    }
                }
            }
        }
        // bold: **foo** — must check before single-star em.
        if c == b'*' && bytes.get(i + 1).copied() == Some(b'*') {
            if let Some(end) = find_after(s, i + 2, "**") {
                flush_text(&mut out, &s[text_start..i]);
                let body = &s[i + 2..end];
                out.push(json!({
                    "type": "text",
                    "text": body,
                    "marks": [{"type": "strong"}],
                }));
                i = end + 2;
                text_start = i;
                continue;
            }
        }
        // em: *foo* — single-star, not part of a `**` pair. Reject empty
        // body so that a stray `**` (no closing match for bold) doesn't get
        // consumed as an em with an empty body.
        if c == b'*' {
            if let Some(end) = find_after(s, i + 1, "*") {
                let before_ok = i == 0 || bytes[i - 1] != b'*';
                let after_ok = bytes.get(end + 1).copied() != Some(b'*');
                let body = &s[i + 1..end];
                if before_ok && after_ok && !body.is_empty() {
                    flush_text(&mut out, &s[text_start..i]);
                    out.push(json!({
                        "type": "text",
                        "text": body,
                        "marks": [{"type": "em"}],
                    }));
                    i = end + 1;
                    text_start = i;
                    continue;
                }
            }
        }
        i += 1;
    }
    flush_text(&mut out, &s[text_start..]);
    out
}

fn find_after(s: &str, from: usize, needle: &str) -> Option<usize> {
    if from > s.len() {
        return None;
    }
    s[from..].find(needle).map(|i| i + from)
}

fn flush_text(out: &mut Vec<Json>, slice: &str) {
    if !slice.is_empty() {
        out.push(json!({ "type": "text", "text": slice }));
    }
}

// --- ADF node constructors -------------------------------------------------

fn adf_paragraph(inline: Vec<Json>) -> Json {
    json!({
        "type": "paragraph",
        "content": inline,
    })
}

fn adf_code_block(language: Option<&str>, body: &str) -> Json {
    let mut node = serde_json::Map::new();
    node.insert("type".into(), Json::String("codeBlock".into()));
    if let Some(lang) = language {
        node.insert(
            "attrs".into(),
            json!({ "language": lang }),
        );
    }
    node.insert(
        "content".into(),
        json!([{ "type": "text", "text": body }]),
    );
    Json::Object(node)
}

fn adf_list(kind: &str, items: Vec<String>) -> Json {
    let nodes: Vec<Json> = items
        .into_iter()
        .map(|item| {
            json!({
                "type": "listItem",
                "content": [adf_paragraph(parse_inlines(&item))],
            })
        })
        .collect();
    json!({ "type": kind, "content": nodes })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn first_block_type(adf: &Json) -> &str {
        adf["content"][0]["type"].as_str().unwrap_or("")
    }

    #[test]
    fn empty_input_renders_single_empty_paragraph() {
        let adf = markdown_to_adf("");
        assert_eq!(adf["version"], 1);
        assert_eq!(adf["type"], "doc");
        assert_eq!(first_block_type(&adf), "paragraph");
        assert!(adf["content"][0]["content"].as_array().unwrap().is_empty());
    }

    #[test]
    fn single_paragraph() {
        let adf = markdown_to_adf("hello world");
        assert_eq!(first_block_type(&adf), "paragraph");
        assert_eq!(adf["content"][0]["content"][0]["text"], "hello world");
    }

    #[test]
    fn paragraph_with_inline_marks() {
        let adf = markdown_to_adf("**bold** and *em* and `code`");
        let inline = adf["content"][0]["content"].as_array().unwrap();
        let kinds: Vec<&str> = inline
            .iter()
            .map(|n| {
                n["marks"][0]["type"]
                    .as_str()
                    .unwrap_or("")
            })
            .collect();
        assert!(kinds.contains(&"strong"));
        assert!(kinds.contains(&"em"));
        assert!(kinds.contains(&"code"));
    }

    #[test]
    fn paragraph_with_link() {
        let adf = markdown_to_adf("see [docs](https://example.com)");
        let inline = adf["content"][0]["content"].as_array().unwrap();
        let link = inline
            .iter()
            .find(|n| n["marks"][0]["type"] == "link")
            .unwrap();
        assert_eq!(link["text"], "docs");
        assert_eq!(link["marks"][0]["attrs"]["href"], "https://example.com");
    }

    #[test]
    fn fenced_code_block_with_language() {
        let adf = markdown_to_adf("```rust\nfn main() {}\n```");
        assert_eq!(first_block_type(&adf), "codeBlock");
        assert_eq!(adf["content"][0]["attrs"]["language"], "rust");
        assert_eq!(
            adf["content"][0]["content"][0]["text"],
            "fn main() {}"
        );
    }

    #[test]
    fn fenced_code_block_without_language() {
        let adf = markdown_to_adf("```\nplain\n```");
        assert_eq!(first_block_type(&adf), "codeBlock");
        // No `attrs` key when language is unspecified.
        assert!(adf["content"][0].get("attrs").is_none());
    }

    #[test]
    fn bullet_list() {
        let adf = markdown_to_adf("- first\n- second");
        assert_eq!(first_block_type(&adf), "bulletList");
        let items = adf["content"][0]["content"].as_array().unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["type"], "listItem");
        assert_eq!(
            items[1]["content"][0]["content"][0]["text"],
            "second"
        );
    }

    #[test]
    fn ordered_list() {
        let adf = markdown_to_adf("1. first\n2. second");
        assert_eq!(first_block_type(&adf), "orderedList");
    }

    #[test]
    fn multiple_blocks() {
        let md = "intro paragraph\n\n```rust\nlet x = 1;\n```\n\n- bullet a\n- bullet b\n\ntail paragraph";
        let adf = markdown_to_adf(md);
        let kinds: Vec<&str> = adf["content"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["type"].as_str().unwrap())
            .collect();
        assert_eq!(kinds, vec!["paragraph", "codeBlock", "bulletList", "paragraph"]);
    }

    #[test]
    fn unsupported_features_fall_through_as_plain_paragraph() {
        // Headings and blockquotes are not supported in the Phase 4 subset.
        // The converter doesn't crash — it emits the literal text as a
        // paragraph, which is the contract documented in plan §7 #4.
        let adf = markdown_to_adf("# heading\n\n> quoted");
        let kinds: Vec<&str> = adf["content"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["type"].as_str().unwrap())
            .collect();
        assert_eq!(kinds, vec!["paragraph", "paragraph"]);
        assert_eq!(adf["content"][0]["content"][0]["text"], "# heading");
        assert_eq!(adf["content"][1]["content"][0]["text"], "> quoted");
    }

    #[test]
    fn unclosed_inline_does_not_crash() {
        // `code without close, **bold without close — must not be parsed as
        // marked spans, and must not crash. The exact node count depends on
        // how the scanner advances past the partial sentinel; the contract
        // is "no `marks` key on any inline node, and the joined text equals
        // the input".
        let input = "hello `unclosed and **also unclosed";
        let adf = markdown_to_adf(input);
        assert_eq!(first_block_type(&adf), "paragraph");
        let inline = adf["content"][0]["content"].as_array().unwrap();
        let joined: String = inline
            .iter()
            .map(|n| n["text"].as_str().unwrap_or(""))
            .collect();
        assert_eq!(joined, input);
        for n in inline {
            assert!(
                n.get("marks").is_none(),
                "unclosed sentinel should not produce marked spans: {n}"
            );
        }
    }
}
