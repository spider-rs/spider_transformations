//! PowerPoint (.pptx) to markdown conversion.
//!
//! Opens the pptx ZIP archive, iterates slides in natural order, and extracts
//! text + tables from each slide's XML. All paths return `Result` — no panics.

use super::DocumentError;
use quick_xml::events::Event;
use quick_xml::Reader;
use std::fmt::Write;
use std::io::Cursor;

/// Maximum XML events per slide before bailing out.
const MAX_EVENTS_PER_SLIDE: usize = 500_000;

/// Convert raw pptx bytes to a markdown string.
pub(crate) fn to_markdown(bytes: &[u8]) -> Result<String, DocumentError> {
    let cursor = Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor)?;

    // Collect slide file names and sort naturally
    let mut slide_names: Vec<String> = (0..archive.len())
        .filter_map(|i| {
            let name = archive.name_for_index(i)?;
            if name.starts_with("ppt/slides/slide") && name.ends_with(".xml") {
                Some(name.to_string())
            } else {
                None
            }
        })
        .collect();

    natural_sort_slides(&mut slide_names);

    if slide_names.is_empty() {
        return Ok(String::new());
    }

    let mut out = String::with_capacity(bytes.len() / 8);

    for (i, slide_name) in slide_names.iter().enumerate() {
        let slide_file = match archive.by_name(slide_name) {
            Ok(f) => f,
            Err(_) => continue, // Skip slides that can't be read
        };

        if i > 0 {
            out.push('\n');
        }
        let _ = write!(out, "## Slide {}\n\n", i + 1);

        // Parse slide XML; errors in one slide don't block others
        if let Err(_) = parse_slide(slide_file, &mut out) {
            continue;
        }
    }

    Ok(out)
}

/// Natural sort for slide filenames: slide2 before slide10.
fn natural_sort_slides(names: &mut [String]) {
    names.sort_by(|a, b| {
        let num_a = extract_slide_number(a);
        let num_b = extract_slide_number(b);
        num_a.cmp(&num_b)
    });
}

/// Extract the numeric part from a slide filename like "ppt/slides/slide12.xml".
fn extract_slide_number(name: &str) -> u32 {
    name.strip_prefix("ppt/slides/slide")
        .and_then(|rest| rest.strip_suffix(".xml"))
        .and_then(|num_str| num_str.parse().ok())
        .unwrap_or(0)
}

/// Parse a single slide's XML and append text/tables to the output.
fn parse_slide<R: std::io::Read>(reader_source: R, out: &mut String) -> Result<(), DocumentError> {
    let mut reader = Reader::from_reader(std::io::BufReader::new(reader_source));
    reader.config_mut().trim_text(true);

    let mut buf = Vec::with_capacity(4096);

    // Text state
    let mut in_text_element = false;
    let mut paragraph_text = String::new();

    // Table state
    let mut in_table = false;
    let mut table_rows: Vec<Vec<String>> = Vec::new();
    let mut current_row: Vec<String> = Vec::new();
    let mut current_cell = String::new();
    let mut in_table_cell = false;

    let mut event_count: usize = 0;

    loop {
        if event_count >= MAX_EVENTS_PER_SLIDE {
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
                        paragraph_text.clear();
                    }
                    b"t" => {
                        in_text_element = true;
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

            Ok(Event::Text(ref e)) if in_text_element => {
                if let Ok(text) = e.unescape() {
                    let text_str = text.as_ref();
                    if !text_str.is_empty() {
                        if in_table_cell {
                            current_cell.push_str(text_str);
                        } else {
                            paragraph_text.push_str(text_str);
                        }
                    }
                }
            }

            Ok(Event::End(ref e)) => {
                let name = e.name();
                let local = local_name(name.as_ref());
                match local {
                    b"t" => {
                        in_text_element = false;
                    }
                    b"p" => {
                        if in_table_cell {
                            // Multiple paragraphs within a cell — join with space
                            if !current_cell.is_empty() && !paragraph_text.is_empty() {
                                current_cell.push(' ');
                            }
                            current_cell.push_str(&paragraph_text);
                        } else if !paragraph_text.is_empty() {
                            out.push_str(&paragraph_text);
                            out.push_str("\n\n");
                        }

                        paragraph_text.clear();
                    }
                    b"tc" if in_table => {
                        in_table_cell = false;
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
                        emit_table(out, &table_rows);
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

    Ok(())
}

/// Extract the local name from a possibly-namespaced XML tag.
fn local_name(full: &[u8]) -> &[u8] {
    match full.iter().position(|&b| b == b':') {
        Some(pos) => &full[pos + 1..],
        None => full,
    }
}

/// Emit a markdown table from collected rows.
fn emit_table(out: &mut String, rows: &[Vec<String>]) {
    if rows.is_empty() {
        return;
    }

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
    fn test_extract_slide_number() {
        assert_eq!(extract_slide_number("ppt/slides/slide1.xml"), 1);
        assert_eq!(extract_slide_number("ppt/slides/slide12.xml"), 12);
        assert_eq!(extract_slide_number("ppt/slides/slide.xml"), 0);
        assert_eq!(extract_slide_number("other.xml"), 0);
    }

    #[test]
    fn test_natural_sort_slides() {
        let mut names = vec![
            "ppt/slides/slide10.xml".to_string(),
            "ppt/slides/slide2.xml".to_string(),
            "ppt/slides/slide1.xml".to_string(),
            "ppt/slides/slide3.xml".to_string(),
        ];
        natural_sort_slides(&mut names);
        assert_eq!(
            names,
            vec![
                "ppt/slides/slide1.xml",
                "ppt/slides/slide2.xml",
                "ppt/slides/slide3.xml",
                "ppt/slides/slide10.xml",
            ]
        );
    }

    #[test]
    fn test_local_name() {
        assert_eq!(local_name(b"a:t"), b"t");
        assert_eq!(local_name(b"a:tbl"), b"tbl");
        assert_eq!(local_name(b"body"), b"body");
    }

    #[test]
    fn test_emit_table_empty() {
        let mut out = String::new();
        emit_table(&mut out, &[]);
        assert!(out.is_empty());
    }
}
