//! Excel (.xlsx) to markdown conversion.
//!
//! Uses `calamine` for reading workbook data. All paths return `Result` —
//! no `.unwrap()`, no index access, no panics.

use super::DocumentError;
use calamine::{Data, Reader, Xlsx};
use std::fmt::Write;
use std::io::Cursor;

/// Convert raw xlsx bytes to a markdown string.
///
/// Each worksheet becomes a `## SheetName` heading followed by a markdown table.
pub(crate) fn to_markdown(bytes: &[u8]) -> Result<String, DocumentError> {
    let cursor = Cursor::new(bytes);
    let mut workbook: Xlsx<_> = Xlsx::new(cursor)?;

    let sheet_names = workbook.sheet_names().to_owned();

    if sheet_names.is_empty() {
        return Ok(String::new());
    }

    // Pre-allocate with a reasonable estimate
    let mut out = String::with_capacity(bytes.len() / 4);

    for (idx, sheet_name) in sheet_names.iter().enumerate() {
        let range = match workbook.worksheet_range(sheet_name) {
            Ok(r) => r,
            Err(_) => continue,
        };

        let (rows, cols) = range.get_size();
        if rows == 0 || cols == 0 {
            continue;
        }

        // Sheet heading
        if idx > 0 {
            out.push('\n');
        }
        let _ = write!(out, "## {}\n\n", sheet_name);

        // Build markdown table from rows iterator (no index access)
        let mut first_row = true;
        for row in range.rows() {
            out.push('|');
            for cell in row {
                out.push(' ');
                write_cell_value(&mut out, cell);
                out.push_str(" |");
            }
            out.push('\n');

            // After the first row (header), emit the separator
            if first_row {
                out.push('|');
                for _ in 0..row.len() {
                    out.push_str(" --- |");
                }
                out.push('\n');
                first_row = false;
            }
        }

        out.push('\n');
    }

    Ok(out)
}

/// Write a cell value to the output string, escaping pipe characters.
fn write_cell_value(out: &mut String, cell: &Data) {
    match cell {
        Data::Empty => {}
        Data::String(s) => {
            // Escape pipe characters inside cell values
            if s.contains('|') {
                out.push_str(&s.replace('|', "\\|"));
            } else {
                out.push_str(s);
            }
        }
        Data::Int(n) => {
            let _ = write!(out, "{}", n);
        }
        Data::Float(f) => {
            let _ = write!(out, "{}", f);
        }
        Data::Bool(b) => {
            out.push_str(if *b { "true" } else { "false" });
        }
        Data::DateTime(dt) => {
            let _ = write!(out, "{}", dt);
        }
        Data::DateTimeIso(s) => {
            out.push_str(s);
        }
        Data::DurationIso(s) => {
            out.push_str(s);
        }
        Data::Error(e) => {
            let _ = write!(out, "{}", e);
        }
    }
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
    fn test_cell_pipe_escaping() {
        let mut out = String::new();
        write_cell_value(&mut out, &Data::String("hello|world".into()));
        assert_eq!(out, "hello\\|world");
    }

    #[test]
    fn test_cell_types() {
        let mut out = String::new();

        write_cell_value(&mut out, &Data::Int(42));
        assert_eq!(out, "42");

        out.clear();
        write_cell_value(&mut out, &Data::Bool(true));
        assert_eq!(out, "true");

        out.clear();
        write_cell_value(&mut out, &Data::Empty);
        assert!(out.is_empty());
    }
}
