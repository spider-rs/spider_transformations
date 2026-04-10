//! Word (.docx) to markdown conversion.
//!
//! Opens the docx ZIP archive, reads `word/document.xml`, and parses it with
//! an event-driven `quick-xml` reader. All paths return `Result` — no panics.

use super::DocumentError;
use quick_xml::events::Event;
use quick_xml::Reader;
use std::io::Cursor;

/// Maximum XML events to process before bailing out (prevents infinite loops
/// on malformed files).
const MAX_EVENTS: usize = 1_000_000;

/// Convert raw docx bytes to a markdown string.
pub(crate) fn to_markdown(bytes: &[u8]) -> Result<String, DocumentError> {
    let cursor = Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor)?;
    let doc_entry = archive.by_name("word/document.xml")?;

    let mut reader = Reader::from_reader(std::io::BufReader::new(doc_entry));
    reader.config_mut().trim_text(true);

    let mut out = String::with_capacity(bytes.len() / 8);
    let mut buf = Vec::with_capacity(4096);

    // State tracking
    let mut in_paragraph = false;
    let mut in_run = false;
    let mut in_text = false;
    let mut heading_level: Option<u8> = None;
    let mut is_bold = false;
    let mut is_italic = false;
    let mut paragraph_text = String::new();

    // Table state
    let mut in_table = false;
    let mut table_rows: Vec<Vec<String>> = Vec::new();
    let mut current_row: Vec<String> = Vec::new();
    let mut current_cell = String::new();
    let mut in_table_cell = false;

    let mut event_count: usize = 0;

    loop {
        if event_count >= MAX_EVENTS {
            break;
        }
        event_count += 1;

        match reader.read_event_into(&mut buf) {
            Ok(Event::Eof) => break,

            Ok(Event::Start(ref e)) => {
                let name = e.name();
                let local = local_name(name.as_ref());
                match local {
                    b"p" => {
                        in_paragraph = true;
                        paragraph_text.clear();
                        heading_level = None;
                        is_bold = false;
                        is_italic = false;
                    }
                    b"r" => {
                        in_run = true;
                    }
                    b"t" => {
                        in_text = true;
                    }
                    b"pStyle" if in_paragraph => {
                        // Detect heading level from <w:pStyle w:val="Heading1"/>
                        heading_level = parse_heading_level(e);
                    }
                    b"b" if in_run => {
                        is_bold = true;
                    }
                    b"i" if in_run => {
                        is_italic = true;
                    }
                    b"tbl" => {
                        in_table = true;
                        table_rows.clear();
                    }
                    b"tr" if in_table => {
                        current_row.clear();
                    }
                    b"tc" if in_table => {
                        in_table_cell = true;
                        current_cell.clear();
                    }
                    _ => {}
                }
            }

            Ok(Event::Empty(ref e)) => {
                let name = e.name();
                let local = local_name(name.as_ref());
                match local {
                    b"pStyle" if in_paragraph => {
                        heading_level = parse_heading_level(e);
                    }
                    b"b" if in_run => {
                        is_bold = true;
                    }
                    b"i" if in_run => {
                        is_italic = true;
                    }
                    b"br" if in_paragraph => {
                        paragraph_text.push('\n');
                    }
                    _ => {}
                }
            }

            Ok(Event::Text(ref e)) if in_text => {
                if let Ok(text) = e.unescape() {
                    let text_str = text.as_ref();
                    if !text_str.is_empty() {
                        if in_table_cell {
                            // Table cell text
                            if is_bold {
                                current_cell.push_str("**");
                            }
                            if is_italic {
                                current_cell.push('*');
                            }
                            current_cell.push_str(text_str);
                            if is_italic {
                                current_cell.push('*');
                            }
                            if is_bold {
                                current_cell.push_str("**");
                            }
                        } else {
                            // Regular paragraph text
                            if is_bold {
                                paragraph_text.push_str("**");
                            }
                            if is_italic {
                                paragraph_text.push('*');
                            }
                            paragraph_text.push_str(text_str);
                            if is_italic {
                                paragraph_text.push('*');
                            }
                            if is_bold {
                                paragraph_text.push_str("**");
                            }
                        }
                    }
                }
            }

            Ok(Event::End(ref e)) => {
                let name = e.name();
                let local = local_name(name.as_ref());
                match local {
                    b"t" => {
                        in_text = false;
                    }
                    b"r" => {
                        in_run = false;
                        is_bold = false;
                        is_italic = false;
                    }
                    b"p" => {
                        in_paragraph = false;

                        if in_table_cell {
                            // Append paragraph text to current cell
                            if !current_cell.is_empty() && !paragraph_text.is_empty() {
                                current_cell.push(' ');
                            }
                            current_cell.push_str(&paragraph_text);
                        } else if !paragraph_text.is_empty() {
                            // Emit the paragraph
                            emit_paragraph(&mut out, &paragraph_text, heading_level);
                        }

                        paragraph_text.clear();
                    }
                    b"tc" if in_table => {
                        in_table_cell = false;
                        // Escape pipe characters in cell content
                        let cell = if current_cell.contains('|') {
                            current_cell.replace('|', "\\|")
                        } else {
                            current_cell.clone()
                        };
                        current_row.push(cell);
                        current_cell.clear();
                    }
                    b"tr" if in_table => {
                        table_rows.push(current_row.clone());
                        current_row.clear();
                    }
                    b"tbl" => {
                        in_table = false;
                        emit_table(&mut out, &table_rows);
                        table_rows.clear();
                    }
                    _ => {}
                }
            }

            Ok(_) => {}
            Err(_) => break,
        }

        buf.clear();
    }

    Ok(out)
}

/// Extract the local name from a possibly-namespaced XML tag.
/// e.g. `w:p` → `p`, `a:t` → `t`, `p` → `p`
fn local_name(full: &[u8]) -> &[u8] {
    match full.iter().position(|&b| b == b':') {
        Some(pos) => &full[pos + 1..],
        None => full,
    }
}

/// Try to parse a heading level from a pStyle element's val attribute.
/// Looks for patterns like "Heading1", "Heading2", etc.
fn parse_heading_level(e: &quick_xml::events::BytesStart<'_>) -> Option<u8> {
    for attr in e.attributes().flatten() {
        let key = local_name(attr.key.as_ref());
        if key == b"val" {
            if let Ok(val) = std::str::from_utf8(&attr.value) {
                // Match "Heading1" through "Heading6" (case-insensitive prefix)
                let lower = val.to_ascii_lowercase();
                if let Some(rest) = lower.strip_prefix("heading") {
                    if let Ok(level) = rest.parse::<u8>() {
                        if (1..=6).contains(&level) {
                            return Some(level);
                        }
                    }
                }
            }
        }
    }
    None
}

/// Emit a paragraph to the output with optional heading prefix.
fn emit_paragraph(out: &mut String, text: &str, heading_level: Option<u8>) {
    if let Some(level) = heading_level {
        for _ in 0..level {
            out.push('#');
        }
        out.push(' ');
    }
    out.push_str(text);
    out.push_str("\n\n");
}

/// Emit a markdown table from collected rows.
fn emit_table(out: &mut String, rows: &[Vec<String>]) {
    if rows.is_empty() {
        return;
    }

    // Determine max column count across all rows
    let max_cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    if max_cols == 0 {
        return;
    }

    for (i, row) in rows.iter().enumerate() {
        out.push('|');
        for col_idx in 0..max_cols {
            out.push(' ');
            if let Some(cell) = row.get(col_idx) {
                out.push_str(cell);
            }
            out.push_str(" |");
        }
        out.push('\n');

        // Separator after first row (header)
        if i == 0 {
            out.push('|');
            for _ in 0..max_cols {
                out.push_str(" --- |");
            }
            out.push('\n');
        }
    }

    out.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_garbage_bytes_no_panic() {
        assert!(to_markdown(&[]).is_err());
        assert!(to_markdown(&[0xFF; 256]).is_err());
    }

    #[test]
    fn test_local_name() {
        assert_eq!(local_name(b"w:p"), b"p");
        assert_eq!(local_name(b"a:t"), b"t");
        assert_eq!(local_name(b"body"), b"body");
    }

    #[test]
    fn test_emit_paragraph_heading() {
        let mut out = String::new();
        emit_paragraph(&mut out, "Title", Some(1));
        assert_eq!(out, "# Title\n\n");
    }

    #[test]
    fn test_emit_paragraph_plain() {
        let mut out = String::new();
        emit_paragraph(&mut out, "Hello world", None);
        assert_eq!(out, "Hello world\n\n");
    }

    #[test]
    fn test_emit_table_empty() {
        let mut out = String::new();
        emit_table(&mut out, &[]);
        assert!(out.is_empty());
    }

    #[test]
    fn test_emit_table_basic() {
        let mut out = String::new();
        let rows = vec![
            vec!["A".to_string(), "B".to_string()],
            vec!["1".to_string(), "2".to_string()],
        ];
        emit_table(&mut out, &rows);
        assert!(out.contains("| A | B |"));
        assert!(out.contains("| --- | --- |"));
        assert!(out.contains("| 1 | 2 |"));
    }

    #[test]
    fn test_emit_table_ragged_rows() {
        let mut out = String::new();
        let rows = vec![
            vec!["A".to_string(), "B".to_string(), "C".to_string()],
            vec!["1".to_string()], // fewer columns
        ];
        emit_table(&mut out, &rows);
        // Should not panic, pads missing cells
        assert!(out.contains("| A | B | C |"));
        assert!(out.contains("| 1 |  |  |"));
    }
}
