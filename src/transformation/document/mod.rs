//! Office document (xlsx, docx, pptx) to markdown conversion.
//!
//! All conversions are panic-free: errors are converted to `None` at the
//! public boundary so callers fall through to the existing binary-file path.

mod docx;
mod pptx;
mod xlsx;

/// Recognized Office document types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DocumentType {
    Xlsx,
    Docx,
    Pptx,
}

/// Internal error type — never exposed outside this module.
#[derive(Debug)]
pub(crate) enum DocumentError {
    Zip(zip::result::ZipError),
    Xml(quick_xml::Error),
    XmlAttr(quick_xml::events::attributes::AttrError),
    Calamine(calamine::Error),
    CalamineXlsx(calamine::XlsxError),
    Io(std::io::Error),
}

impl From<zip::result::ZipError> for DocumentError {
    fn from(e: zip::result::ZipError) -> Self {
        DocumentError::Zip(e)
    }
}

impl From<quick_xml::Error> for DocumentError {
    fn from(e: quick_xml::Error) -> Self {
        DocumentError::Xml(e)
    }
}

impl From<quick_xml::events::attributes::AttrError> for DocumentError {
    fn from(e: quick_xml::events::attributes::AttrError) -> Self {
        DocumentError::XmlAttr(e)
    }
}

impl From<calamine::Error> for DocumentError {
    fn from(e: calamine::Error) -> Self {
        DocumentError::Calamine(e)
    }
}

impl From<calamine::XlsxError> for DocumentError {
    fn from(e: calamine::XlsxError) -> Self {
        DocumentError::CalamineXlsx(e)
    }
}

impl From<std::io::Error> for DocumentError {
    fn from(e: std::io::Error) -> Self {
        DocumentError::Io(e)
    }
}

/// Decode + entity-unescape an XML text event, matching the prior
/// `BytesText::unescape()` semantics removed in quick-xml 0.39.
pub(super) fn decode_unescape(e: &quick_xml::events::BytesText<'_>) -> Option<String> {
    let decoded = e.decode().ok()?;
    let unescaped = quick_xml::escape::unescape(&decoded).ok()?;
    Some(unescaped.into_owned())
}

/// ZIP local-file-header magic bytes.
const ZIP_MAGIC: [u8; 4] = [0x50, 0x4B, 0x03, 0x04];

/// Detect the Office document type from raw bytes.
///
/// Returns `None` for non-ZIP or unrecognized ZIP archives.
fn detect_document_type(bytes: &[u8]) -> Option<DocumentType> {
    if bytes.len() < 4 || bytes[..4] != ZIP_MAGIC {
        return None;
    }

    let cursor = std::io::Cursor::new(bytes);
    let archive = zip::ZipArchive::new(cursor).ok()?;

    let mut has_xl = false;
    let mut has_word = false;
    let mut has_ppt = false;

    for i in 0..archive.len() {
        if let Some(name) = archive.name_for_index(i) {
            if name.starts_with("xl/") {
                has_xl = true;
                break;
            } else if name.starts_with("word/") {
                has_word = true;
                break;
            } else if name.starts_with("ppt/") {
                has_ppt = true;
                break;
            }
        }
    }

    if has_xl {
        Some(DocumentType::Xlsx)
    } else if has_word {
        Some(DocumentType::Docx)
    } else if has_ppt {
        Some(DocumentType::Pptx)
    } else {
        None
    }
}

/// Try to convert binary bytes to markdown if they are a recognized Office
/// document. Returns `None` on any error or if the bytes are not a document.
pub(crate) fn try_convert_document(bytes: &[u8]) -> Option<String> {
    let doc_type = detect_document_type(bytes)?;
    let result = match doc_type {
        DocumentType::Xlsx => xlsx::to_markdown(bytes),
        DocumentType::Docx => docx::to_markdown(bytes),
        DocumentType::Pptx => pptx::to_markdown(bytes),
    };
    result.ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_empty_bytes() {
        assert!(detect_document_type(&[]).is_none());
    }

    #[test]
    fn test_detect_non_zip() {
        assert!(detect_document_type(b"hello world this is not a zip").is_none());
    }

    #[test]
    fn test_detect_truncated_zip_magic() {
        assert!(detect_document_type(&[0x50, 0x4B]).is_none());
    }

    #[test]
    fn test_try_convert_garbage_no_panic() {
        // Must not panic on arbitrary bytes
        assert!(try_convert_document(&[]).is_none());
        assert!(try_convert_document(&[0xFF; 100]).is_none());
        assert!(try_convert_document(b"not a document at all").is_none());
    }

    #[test]
    fn test_try_convert_truncated_zip_no_panic() {
        // Valid ZIP magic but truncated — must not panic
        let truncated = &[0x50, 0x4B, 0x03, 0x04, 0x00, 0x00];
        assert!(try_convert_document(truncated).is_none());
    }
}
