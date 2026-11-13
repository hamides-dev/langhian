#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineSpan {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarkdownBlock {
    Heading {
        level: u8,
        spans: Vec<InlineSpan>,
    },
    ListItem {
        marker: String,
        spans: Vec<InlineSpan>,
    },
    Blockquote {
        spans: Vec<InlineSpan>,
    },
    CodeBlock {
        lang: Option<String>,
        code: String,
    },
    Paragraph {
        spans: Vec<InlineSpan>,
    },
    HorizontalRule,
}

pub fn contains_markdown_structure(text: &str) -> bool {
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("# ")
            || trimmed.starts_with("## ")
            || trimmed.starts_with("### ")
            || trimmed.starts_with("#### ")
            || trimmed.starts_with("- ")
            || trimmed.starts_with("* ")
            || trimmed.starts_with("> ")
            || trimmed.starts_with("```")
            || is_ordered_list_line(trimmed).is_some()
        {
            return true;
        }
    }
    text.contains("**") || (text.matches('`').count() >= 2)
}

pub fn parse_markdown_blocks(input: &str) -> Vec<MarkdownBlock> {
    let mut blocks = Vec::new();
    let lines: Vec<&str> = input.lines().collect();
    let mut i = 0usize;

    while i < lines.len() {
        let raw_line = lines[i];
        let trimmed = raw_line.trim();

        if trimmed.is_empty() {
            i += 1;
            continue;
        }

        // Fenced code block
        if let Some(after_ticks) = trimmed.strip_prefix("```") {
            let lang = {
                let l = after_ticks.trim();
                if l.is_empty() {
                    None
                } else {
                    Some(l.to_string())
                }
            };
            let mut code_lines = Vec::new();
            i += 1;
            while i < lines.len() {
                if lines[i].trim_start().starts_with("```") {
                    i += 1;
                    break;
                }
                code_lines.push(lines[i]);
                i += 1;
            }
            blocks.push(MarkdownBlock::CodeBlock {
                lang,
                code: code_lines.join("\n"),
            });
            continue;
        }

        // Horizontal rule
        if trimmed == "---" || trimmed == "***" || trimmed == "___" {
            blocks.push(MarkdownBlock::HorizontalRule);
            i += 1;
            continue;
        }

        // ATX Heading (#..####)
        if let Some((level, heading_body)) = parse_heading_line(trimmed) {
            blocks.push(MarkdownBlock::Heading {
                level,
                spans: parse_inline_spans(heading_body),
            });
            i += 1;
            continue;
        }

        // Blockquote
        if let Some(quote_body) = trimmed.strip_prefix("> ") {
            let mut quote_lines = vec![quote_body.trim()];
            i += 1;
            while i < lines.len() {
                let next_trimmed = lines[i].trim();
                if let Some(cont) = next_trimmed.strip_prefix("> ") {
                    quote_lines.push(cont.trim());
                    i += 1;
                } else {
                    break;
                }
            }
            blocks.push(MarkdownBlock::Blockquote {
                spans: parse_inline_spans(&quote_lines.join("\n")),
            });
            continue;
        }

        // Unordered list
        if let Some(rest) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
            .or_else(|| trimmed.strip_prefix("• "))
        {
            blocks.push(MarkdownBlock::ListItem {
                marker: "•".to_string(),
                spans: parse_inline_spans(rest.trim()),
            });
            i += 1;
            continue;
        }

        // Ordered list
        if let Some((num, rest)) = is_ordered_list_line(trimmed) {
            blocks.push(MarkdownBlock::ListItem {
                marker: format!("{num}."),
                spans: parse_inline_spans(rest.trim()),
            });
            i += 1;
            continue;
        }

        // Paragraph: coalesce contiguous plain lines until blank/block boundary
        let mut para_lines = vec![trimmed];
        i += 1;
        while i < lines.len() {
            let next = lines[i].trim();
            if next.is_empty()
                || next.starts_with("```")
                || next.starts_with("> ")
                || next.starts_with("- ")
                || next.starts_with("* ")
                || next == "---"
                || parse_heading_line(next).is_some()
                || is_ordered_list_line(next).is_some()
            {
                break;
            }
            para_lines.push(next);
            i += 1;
        }

        blocks.push(MarkdownBlock::Paragraph {
            spans: parse_inline_spans(&para_lines.join("\n")),
        });
    }

    blocks
}

fn parse_heading_line(trimmed: &str) -> Option<(u8, &str)> {
    let hashes = trimmed.bytes().take_while(|&b| b == b'#').count();
    if (1..=6).contains(&hashes) {
        let rest = &trimmed[hashes..];
        if let Some(content) = rest.strip_prefix(' ') {
            return Some((hashes as u8, content.trim()));
        }
    }
    None
}

fn is_ordered_list_line(trimmed: &str) -> Option<(&str, &str)> {
    let digits_len = trimmed.bytes().take_while(|b| b.is_ascii_digit()).count();
    if digits_len == 0 || digits_len > 4 {
        return None;
    }
    let after_digits = &trimmed[digits_len..];
    if let Some(rest) = after_digits.strip_prefix(". ") {
        Some((&trimmed[..digits_len], rest))
    } else {
        None
    }
}

pub fn parse_inline_spans(text: &str) -> Vec<InlineSpan> {
    let mut out = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0usize;
    let mut buf = String::new();

    let flush_plain = |out: &mut Vec<InlineSpan>, buf: &mut String| {
        if !buf.is_empty() {
            out.push(InlineSpan {
                text: std::mem::take(buf),
                bold: false,
                italic: false,
                code: false,
            });
        }
    };

    while i < chars.len() {
        // Inline code `...`
        if chars[i] == '`'
            && let Some(end_rel) = chars[i + 1..].iter().position(|&c| c == '`')
        {
            flush_plain(&mut out, &mut buf);
            let inner: String = chars[i + 1..i + 1 + end_rel].iter().collect();
            if !inner.is_empty() {
                out.push(InlineSpan {
                    text: inner,
                    bold: false,
                    italic: false,
                    code: true,
                });
            }
            i += end_rel + 2;
            continue;
        }

        // Bold **...**
        if i + 1 < chars.len()
            && chars[i] == '*'
            && chars[i + 1] == '*'
            && let Some(end_rel) = find_double_star(&chars[i + 2..])
        {
            flush_plain(&mut out, &mut buf);
            let inner: String = chars[i + 2..i + 2 + end_rel].iter().collect();
            if !inner.is_empty() {
                out.push(InlineSpan {
                    text: inner,
                    bold: true,
                    italic: false,
                    code: false,
                });
            }
            i += end_rel + 4;
            continue;
        }

        // Italic *...* (single star, non-space boundary)
        if chars[i] == '*'
            && i + 1 < chars.len()
            && !chars[i + 1].is_whitespace()
            && let Some(end_rel) = chars[i + 1..].iter().position(|&c| c == '*')
        {
            flush_plain(&mut out, &mut buf);
            let inner: String = chars[i + 1..i + 1 + end_rel].iter().collect();
            if !inner.is_empty() {
                out.push(InlineSpan {
                    text: inner,
                    bold: false,
                    italic: true,
                    code: false,
                });
            }
            i += end_rel + 2;
            continue;
        }

        buf.push(chars[i]);
        i += 1;
    }

    flush_plain(&mut out, &mut buf);
    out
}

fn find_double_star(slice: &[char]) -> Option<usize> {
    if slice.len() < 2 {
        return None;
    }
    (0..slice.len() - 1).find(|&idx| slice[idx] == '*' && slice[idx + 1] == '*')
}

/// Converts a Windows `CF_HTML` clipboard payload or HTML selection fragment into Markdown
/// when structural markup (`<h1>`..`<h6>`, `<ul>/<ol>/<li>`, `<blockquote>`, `<pre>`, `<code>`, `<strong>`)
/// is present. Returns `None` if no meaningful structural formatting exists so plain text is used.
pub fn html_fragment_to_markdown(raw_html: &str) -> Option<String> {
    let fragment = extract_html_fragment(raw_html);
    let lower = fragment.to_ascii_lowercase();
    let has_structure = lower.contains("<h1")
        || lower.contains("<h2")
        || lower.contains("<h3")
        || lower.contains("<h4")
        || lower.contains("<li")
        || lower.contains("<blockquote")
        || lower.contains("<pre")
        || lower.contains("<code");

    if !has_structure {
        return None;
    }

    let mut md = String::with_capacity(fragment.len());
    let bytes = fragment.as_bytes();
    let mut i = 0usize;
    let mut list_ordered_counter: Option<usize> = None;
    let mut in_pre = false;
    let mut in_script_or_style = false;

    while i < bytes.len() {
        if bytes[i] == b'<'
            && let Some(rel_end) = fragment[i..].find('>')
        {
            let tag_raw = fragment[i + 1..i + rel_end].trim();
            let tag_lower = tag_raw
                .split_whitespace()
                .next()
                .unwrap_or("")
                .trim_end_matches('/')
                .to_ascii_lowercase();

            match tag_lower.as_str() {
                "script" | "style" => in_script_or_style = true,
                "/script" | "/style" => in_script_or_style = false,
                "h1" => {
                    ensure_block_break(&mut md);
                    md.push_str("# ");
                }
                "h2" => {
                    ensure_block_break(&mut md);
                    md.push_str("## ");
                }
                "h3" => {
                    ensure_block_break(&mut md);
                    md.push_str("### ");
                }
                "h4" | "h5" | "h6" => {
                    ensure_block_break(&mut md);
                    md.push_str("#### ");
                }
                "/h1" | "/h2" | "/h3" | "/h4" | "/h5" | "/h6" => {
                    ensure_double_newline(&mut md);
                }
                "p" | "div" | "section" | "article" => {
                    if !md.is_empty() && !md.ends_with("\n\n") {
                        ensure_block_break(&mut md);
                    }
                }
                "/p" | "/div" => {
                    ensure_double_newline(&mut md);
                }
                "br" => {
                    if !md.ends_with('\n') {
                        md.push('\n');
                    }
                }
                "ul" => {
                    list_ordered_counter = None;
                    ensure_block_break(&mut md);
                }
                "ol" => {
                    list_ordered_counter = Some(1);
                    ensure_block_break(&mut md);
                }
                "/ul" | "/ol" => {
                    list_ordered_counter = None;
                    ensure_double_newline(&mut md);
                }
                "li" => {
                    ensure_single_newline(&mut md);
                    if let Some(n) = &mut list_ordered_counter {
                        md.push_str(&format!("{n}. "));
                        *n += 1;
                    } else {
                        md.push_str("- ");
                    }
                }
                "/li" => {
                    if !md.ends_with('\n') {
                        md.push('\n');
                    }
                }
                "blockquote" => {
                    ensure_block_break(&mut md);
                    md.push_str("> ");
                }
                "/blockquote" => {
                    ensure_double_newline(&mut md);
                }
                "pre" => {
                    in_pre = true;
                    ensure_block_break(&mut md);
                    md.push_str("```\n");
                }
                "/pre" => {
                    in_pre = false;
                    if !md.ends_with('\n') {
                        md.push('\n');
                    }
                    md.push_str("```\n\n");
                }
                "code" if !in_pre => md.push('`'),
                "/code" if !in_pre => md.push('`'),
                "strong" | "b" => md.push_str("**"),
                "/strong" | "/b" => md.push_str("**"),
                "em" | "i" => md.push('*'),
                "/em" | "/i" => md.push('*'),
                "hr" => {
                    ensure_block_break(&mut md);
                    md.push_str("---\n\n");
                }
                _ => {}
            }

            i += rel_end + 1;
            continue;
        }

        // Plain text segment until next '<'
        let next_lt = fragment[i..]
            .find('<')
            .map(|rel| i + rel)
            .unwrap_or(bytes.len());
        if !in_script_or_style {
            let chunk = &fragment[i..next_lt];
            let decoded = decode_html_entities(chunk);
            if in_pre {
                md.push_str(&decoded);
            } else {
                push_collapsed_whitespace(&mut md, &decoded);
            }
        }
        i = next_lt;
    }

    let cleaned = normalize_markdown_whitespace(&md);
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

fn extract_html_fragment(raw: &str) -> &str {
    if let Some(start) = raw.find("<!--StartFragment-->") {
        let content_start = start + "<!--StartFragment-->".len();
        if let Some(end_rel) = raw[content_start..].find("<!--EndFragment-->") {
            return &raw[content_start..content_start + end_rel];
        }
        return &raw[content_start..];
    }
    if let Some(body_start) = raw.to_ascii_lowercase().find("<body")
        && let Some(gt_rel) = raw[body_start..].find('>')
    {
        let start = body_start + gt_rel + 1;
        if let Some(body_end) = raw[start..].to_ascii_lowercase().find("</body>") {
            return &raw[start..start + body_end];
        }
        return &raw[start..];
    }
    raw
}

fn ensure_single_newline(out: &mut String) {
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
}

fn ensure_block_break(out: &mut String) {
    if out.is_empty() {
        return;
    }
    if !out.ends_with('\n') {
        out.push_str("\n\n");
    } else if !out.ends_with("\n\n") {
        out.push('\n');
    }
}

fn ensure_double_newline(out: &mut String) {
    while out.ends_with(' ') {
        out.pop();
    }
    if out.is_empty() {
        return;
    }
    if !out.ends_with('\n') {
        out.push_str("\n\n");
    } else if !out.ends_with("\n\n") {
        out.push('\n');
    }
}

fn push_collapsed_whitespace(out: &mut String, chunk: &str) {
    let mut prev_space = out.is_empty() || out.ends_with([' ', '\n', '\t']) || out.ends_with("# ");
    for ch in chunk.chars() {
        if ch.is_whitespace() {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(ch);
            prev_space = false;
        }
    }
}

fn decode_html_entities(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut i = 0usize;
    while i < input.len() {
        let rest = &input[i..];
        if rest.starts_with('&')
            && let Some(semi) = rest[..rest.len().min(12)].find(';')
        {
            let entity = &rest[1..semi];
            let replacement = match entity {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" | "#39" => Some('\''),
                "nbsp" => Some(' '),
                "ndash" => Some('–'),
                "mdash" => Some('—'),
                "rsquo" | "lsquo" => Some('\''),
                "rdquo" | "ldquo" => Some('"'),
                _ if entity.starts_with("#x") || entity.starts_with("#X") => {
                    u32::from_str_radix(&entity[2..], 16)
                        .ok()
                        .and_then(char::from_u32)
                }
                _ if entity.starts_with('#') => {
                    entity[1..].parse::<u32>().ok().and_then(char::from_u32)
                }
                _ => None,
            };
            if let Some(ch) = replacement {
                out.push(ch);
                i += semi + 1;
                continue;
            }
        }
        if let Some(ch) = rest.chars().next() {
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

fn normalize_markdown_whitespace(raw: &str) -> String {
    let lines: Vec<String> = raw.lines().map(|l| l.trim_end().to_string()).collect();
    let mut collapsed = Vec::new();
    let mut empty_run = 0usize;
    for line in lines {
        if line.is_empty() {
            empty_run += 1;
            if empty_run <= 1 && !collapsed.is_empty() {
                collapsed.push(String::new());
            }
        } else {
            empty_run = 0;
            collapsed.push(line);
        }
    }
    collapsed.join("\n").trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_headings_lists_and_inline_spans() {
        let md = "## Architecture Overview\n\nLanghian uses **zero WebView** with `iced` rendering.\n\n- Fast\n- Clean";
        assert!(contains_markdown_structure(md));
        let blocks = parse_markdown_blocks(md);
        assert_eq!(blocks.len(), 4);
        assert!(matches!(blocks[0], MarkdownBlock::Heading { level: 2, .. }));
        assert!(matches!(blocks[1], MarkdownBlock::Paragraph { .. }));
        assert!(matches!(blocks[2], MarkdownBlock::ListItem { .. }));
        assert!(matches!(blocks[3], MarkdownBlock::ListItem { .. }));
    }

    #[test]
    fn converts_browser_cf_html_heading_and_paragraph_to_markdown() {
        let cf_html = "Version:0.9\r\n<!--StartFragment--><h2>Getting Started</h2><p>Select text &amp; press <strong>Ctrl+Ctrl</strong>.</p><!--EndFragment-->";
        let md = html_fragment_to_markdown(cf_html).expect("structured markdown");
        assert!(md.contains("## Getting Started"));
        assert!(md.contains("Select text & press **Ctrl+Ctrl**."));
    }

    #[test]
    fn skips_plain_unstructured_html_fragment() {
        let cf_html = "<!--StartFragment--><span>just plain words</span><!--EndFragment-->";
        assert!(html_fragment_to_markdown(cf_html).is_none());
    }
}
