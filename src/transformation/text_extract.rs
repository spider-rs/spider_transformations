use html2md::extended::sifter::WhitespaceSifter;
use lol_html::{html_content::TextType, text, RewriteStrSettings};

/// extract the text from HTML document.
///
/// If `custom` ignore tags are provided, they are stripped from the HTML in a
/// first pass before text extraction. This ensures ignored elements' text is
/// never captured, regardless of nesting depth.
pub fn extract_text(html: &str, custom: &Option<std::collections::HashSet<String>>) -> String {
    // Pass 1: strip ignored elements from the HTML so their text cannot leak
    // into the extraction pass. lol_html's element remove() only affects the
    // output bytes; text handlers still fire on removed elements' children.
    // A two-pass approach avoids this entirely.
    let cleaned;
    let html = if let Some(ignore) = custom.as_ref().filter(|s| !s.is_empty()) {
        let tags: Vec<&str> = ignore.iter().map(|s| s.as_str()).collect();
        cleaned = super::content::clean_html_elements(html, tags);
        cleaned.as_str()
    } else {
        html
    };

    // Pass 2: extract text from the (now clean) HTML.
    let mut extracted_text = String::new();

    let element_content_handlers = vec![text!(
        "*:not(script):not(style):not(svg):not(noscript)",
        |text| {
            if let TextType::RCData | TextType::Data = text.text_type() {
                let el_text = text.as_str().trim_start();
                if !el_text.is_empty() {
                    if !extracted_text.ends_with(' ') && !extracted_text.is_empty() {
                        extracted_text.push(' ');
                    }
                    extracted_text.push_str(el_text);
                }
                if text.text_type() == TextType::RCData {
                    extracted_text.push('\n');
                }
            }

            Ok(())
        }
    )];

    let _ = rewrite_str_empty(
        html,
        RewriteStrSettings {
            element_content_handlers,
            ..RewriteStrSettings::default()
        },
    );

    extracted_text.sift()
}

/// extract the text from HTML document with chunk size.
pub async fn extract_text_streaming_with_size(
    html: &str,
    custom: &Option<std::collections::HashSet<String>>,
    chunk_size: usize,
) -> String {
    use spider::tokio_stream::StreamExt;

    if html.is_empty() {
        return Default::default();
    }

    // Pass 1: strip ignored elements so their text cannot leak.
    let cleaned;
    let html = if let Some(ignore) = custom.as_ref().filter(|s| !s.is_empty()) {
        let tags: Vec<&str> = ignore.iter().map(|s| s.as_str()).collect();
        cleaned = super::content::clean_html_elements(html, tags);
        cleaned.as_str()
    } else {
        html
    };

    let (txx, mut rxx) = spider::tokio::sync::mpsc::unbounded_channel();

    // Pass 2: extract text from the clean HTML.
    let mut extracted_text = String::new();
    let mut last_sent_position = 0;

    let element_content_handlers = vec![text!(
        "*:not(script):not(style):not(svg):not(noscript)",
        move |text| {
            if let TextType::RCData | TextType::Data = text.text_type() {
                let el_text = text.as_str().trim_start();

                if !el_text.is_empty() {
                    if !extracted_text.ends_with(' ') && !extracted_text.is_empty() {
                        extracted_text.push(' ');
                    }
                    extracted_text.push_str(el_text);
                }
                if text.text_type() == TextType::RCData {
                    extracted_text.push('\n');
                }

                let new_slice = &extracted_text[last_sent_position..];

                if !new_slice.is_empty() {
                    let _ = txx.send(new_slice.to_string());
                    last_sent_position = extracted_text.len();
                }

                // clear the text tracker
                if extracted_text.len() > 1024 {
                    if !extracted_text.ends_with(' ') {
                        extracted_text.clear();
                        last_sent_position = 0;
                    }
                }
            }

            Ok(())
        }
    )];

    let settings = lol_html::send::RewriteStrSettings {
        element_content_handlers,
        ..lol_html::send::RewriteStrSettings::new_send()
    };

    let mut rewriter = lol_html::send::HtmlRewriter::new(settings.into(), |_c: &[u8]| {});

    let html_bytes = html.as_bytes();
    let chunks = html_bytes.chunks(chunk_size);
    let mut wrote_error = false;

    let mut stream = spider::tokio_stream::iter(chunks).map(Ok::<&[u8], ()>);

    while let Some(chunk) = stream.next().await {
        if let Ok(chunk) = chunk {
            if let Err(_) = rewriter.write(chunk) {
                wrote_error = true;
                break;
            }
        }
    }

    if !wrote_error {
        let _ = rewriter.end();
    }

    let mut rewrited_bytes: String = String::new();

    while let Some(c) = rxx.recv().await {
        rewrited_bytes.push_str(&c);
    }

    rewrited_bytes.sift()
}

/// extract the text from HTML document.
pub async fn extract_text_streaming(
    html: &str,
    custom: &Option<std::collections::HashSet<String>>,
) -> String {
    extract_text_streaming_with_size(html, custom, 8192).await
}

pub fn rewrite_str_empty<'h, 's, H: lol_html::HandlerTypes>(
    html: &str,
    settings: impl Into<lol_html::Settings<'h, 's, H>>,
) -> Result<(), lol_html::errors::RewritingError> {
    let mut rewriter = lol_html::HtmlRewriter::new(settings.into(), |_c: &[u8]| {});
    rewriter.write(html.as_bytes())?;
    rewriter.end()?;
    Ok(())
}
