use crate::html2xml::convert_html_to_xml;
use aho_corasick::AhoCorasick;
use html2md;
use phf::phf_set;
use regex::Regex;
use serde::{Deserialize, Deserializer};
use spider::auto_encoder::is_binary_file;
use spider::lazy_static::lazy_static;
use spider::page::Page;
use spider::url::Url;
use spider::utils::clean_html;

lazy_static! {
    static ref AHO: AhoCorasick = AhoCorasick::new(["\n\n\n", "\n  \n  ", "\n\n\n\n\n"]).unwrap();
    static ref AHO_REPLACEMENTS: [&'static str; 3] = [
        "\n\n",  // Replace triple newlines with two newlines
        "\n\n",  // Replace multiple spaces with two newlines
        "\n\n",  // Replace five newlines with two newlines
    ];
    static ref CLEAN_MARKDOWN_REGEX: Regex =  {
        Regex::new(
            r"(?m)^[ \t]+|[ \t]+$|[ \t]+|\s*\n\s*\n\s*"
        ).unwrap()

    };
    static ref EXAMPLE_URL: Url = Url::parse("https://example.net").expect("invalid url");
}

/// The return format for the content.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReturnFormat {
    #[default]
    /// Default format
    Raw,
    /// Bytes - this does not change the output type and more aligned for what the input is.
    Bytes,
    /// Text
    Text,
    /// Text Mapping
    Html2Text,
    /// Screenshot - does nothing without the 'screenshot' flag.
    Screenshot,
    /// Markdown
    Markdown,
    /// Commonmark
    CommonMark,
    /// XML
    XML,
    /// Empty
    Empty,
}

impl ReturnFormat {
    /// Convert the content from string match
    pub fn from_str(s: &str) -> ReturnFormat {
        match s {
            "text" | "Text" | "TEXT" => ReturnFormat::Text,
            "html2text" | "Html2text" | "HTML2TEXT" | "html_2_text" | "HTML_2_TEXT" => {
                ReturnFormat::Html2Text
            }
            "markdown" | "Markdown" | "MARKDOWN" => ReturnFormat::Markdown,
            "raw" | "RAW" | "Raw" => ReturnFormat::Raw,
            "bytes" | "Bytes" | "BYTES" => ReturnFormat::Bytes,
            "commonmark" | "CommonMark" | "COMMONMARK" => ReturnFormat::CommonMark,
            "xml" | "XML" | "XmL" | "Xml" => ReturnFormat::XML,
            "screenshot" | "screenshots" | "SCREENSHOT" | "SCREENSHOTS" | "Screenshot"
            | "Screenshots" => ReturnFormat::Screenshot,
            "empty" | "Empty" | "EMPTY" => ReturnFormat::Empty,
            _ => ReturnFormat::Raw,
        }
    }
}

impl<'de> Deserialize<'de> for ReturnFormat {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;

        match s.as_ref() {
            "text" | "Text" | "TEXT" => Ok(ReturnFormat::Text),
            "html2text" | "Html2text" | "HTML2TEXT" | "html_2_text" | "HTML_2_TEXT" => {
                Ok(ReturnFormat::Html2Text)
            }
            "markdown" | "Markdown" | "MARKDOWN" => Ok(ReturnFormat::Markdown),
            "raw" | "RAW" | "Raw" => Ok(ReturnFormat::Raw),
            "bytes" | "Bytes" | "BYTES" => Ok(ReturnFormat::Bytes),
            "commonmark" | "CommonMark" | "COMMONMARK" => Ok(ReturnFormat::CommonMark),
            "xml" | "XML" | "XmL" | "Xml" => Ok(ReturnFormat::XML),
            "empty" | "Empty" | "EMPTY" => Ok(ReturnFormat::Empty),
            "screenshot" | "screenshots" | "SCREENSHOT" | "SCREENSHOTS" | "Screenshot"
            | "Screenshots" => Ok(ReturnFormat::Screenshot),
            _ => Ok(ReturnFormat::Raw),
        }
    }
}

/// Transformation configuration adjustments.
#[derive(Debug, Default, Clone, Copy)]
pub struct TransformConfig {
    /// Readability mode.
    pub readability: bool,
    /// The return format to use.
    pub return_format: ReturnFormat,
    /// Filter Images.
    pub filter_images: bool,
    /// Trim the content for LLMs.
    pub clean_html: bool,
    /// Filter svgs.
    pub filter_svg: bool,
    /// Main content for the page. Exclude the nav, footer, and etc.
    pub main_content: bool,
}

/// Select elements to show or hide using a CSS selector.
#[derive(Debug, Default, Clone)]
pub struct SelectorConfiguration {
    /// The root html selector.
    pub root_selector: Option<String>,
    /// Exclude the matching css selector from the output.
    pub exclude_selector: Option<String>,
}

/// The transformation input.
pub struct TransformInput<'a> {
    /// Parsed URL (preferred). If you only have &str, parse once upstream and reuse.
    pub url: Option<&'a url::Url>,
    /// Raw response body bytes (used for binary detection + html decode).
    pub content: &'a [u8],
    /// Optional screenshot bytes (PNG/JPEG/etc). Only used when ReturnFormat::Screenshot.
    pub screenshot_bytes: Option<&'a [u8]>,
    /// Optional encoding hint (e.g. "utf-8"). Borrowed to avoid cloning.
    pub encoding: Option<&'a str>,
    /// Optional selector extraction config (borrowed).
    pub selector_config: Option<&'a SelectorConfiguration>,
    /// Optional ignore tags as borrowed &str slices (avoid Vec<String> clones).
    pub ignore_tags: Option<&'a [&'a str]>,
}

/// is the content html and safe for formatting.
static HTML_TAGS: phf::Set<&'static [u8]> = phf_set! {
    b"<!doctype html",
    b"<html",
    b"<document",
};

/// valid file extensions that will render html from a program
pub static VALID_EXTENSIONS: phf::Set<&'static str> = phf_set! {
    ".html",
    ".htm",
    ".shtml",
    ".asp",
    ".aspx",
    ".php",
    ".jps",
    ".jpsx",
    ".jsp",
    ".cfm",
    ".xhtml",
    ".rhtml",
    ".phtml",
    ".erb",
};

/// Check if the content is HTML.
pub fn is_html_content(bytes: &[u8], url: &Url) -> bool {
    let check_bytes = if bytes.len() > 1024 {
        &bytes[..1024]
    } else {
        bytes
    };

    for tag in HTML_TAGS.iter() {
        if check_bytes
            .windows(tag.len())
            .any(|window| window.eq_ignore_ascii_case(tag))
        {
            return true;
        }
    }

    // Heuristic check on URL extension
    if let Some(extension) = url
        .path_segments()
        .and_then(|segments| segments.last().and_then(|s| s.split('.').last()))
    {
        if VALID_EXTENSIONS.contains(extension) {
            return true;
        }
    }
    false
}

/// clean the markdown with aho. This does a triple pass across the content.
pub fn aho_clean_markdown(html: &str) -> String {
    // handle the error on replace all
    // if the content is small just use an aho replacement
    if html.len() <= 40 {
        match AHO.try_replace_all(html, &*AHO_REPLACEMENTS) {
            Ok(r) => r,
            _ => html.into(),
        }
    } else {
        // regex smooth clean multiple
        let cleaned_html = CLEAN_MARKDOWN_REGEX.replace_all(html, |caps: &regex::Captures| {
            let matched = match caps.get(0) {
                Some(m) => m.as_str(),
                _ => Default::default(),
            };
            if matched.contains('\n') && matched.chars().filter(|&c| c == '\n').count() >= 3 {
                "\n\n"
            } else if matched.contains('\n') {
                "\n"
            } else {
                " "
            }
        });

        cleaned_html.into()
    }
}

/// Clean the html elements from the markup.
pub fn clean_html_elements(html: &str, tags: Vec<&str>) -> String {
    use lol_html::{element, rewrite_str, RewriteStrSettings};
    match rewrite_str(
        html,
        RewriteStrSettings {
            element_content_handlers: tags
                .iter()
                .map(|tag| {
                    element!(tag, |el| {
                        el.remove();
                        Ok(())
                    })
                })
                .collect::<Vec<_>>(),
            ..RewriteStrSettings::default()
        },
    ) {
        Ok(r) => r,
        _ => html.into(),
    }
}

/// Buld the static ignore list of html elements.
pub(crate) fn build_static_vector(config: &TransformConfig) -> Vec<&'static str> {
    let mut tags = Vec::new();

    if config.filter_images {
        tags.push("img");
        tags.push("picture");
    }

    if config.filter_svg {
        tags.push("svg");
    }

    if config.main_content {
        tags.push("nav");
        tags.push("header:first-of-type");
        tags.push("footer");
        tags.push("body > aside:not(:first-of-type)");
    }

    tags
}

/// Build a HashSet of tag names from an ignore list.
fn build_ignore_set(ignore: &[String]) -> std::collections::HashSet<String> {
    ignore.iter().map(|s| s.clone()).collect()
}

/// Build a HashSet of tag names from a slice of &str.
fn build_ignore_set_from_strs(ignore: &[&str]) -> std::collections::HashSet<String> {
    ignore.iter().map(|&s| s.to_string()).collect()
}

/// transform the content to markdown shortcut
pub fn transform_markdown(html: &str, commonmark: bool) -> String {
    html2md::rewrite_html_custom_with_url(html, &None, commonmark, &None)
}

/// transform the content to markdown shortcut send
pub async fn transform_markdown_send(html: &str, commonmark: bool) -> String {
    html2md::rewrite_html_custom_with_url_streaming(html, &None, commonmark, &None).await
}

/// transform the content to text raw shortcut
pub fn transform_text(html: &str) -> String {
    super::text_extract::extract_text(html, &Default::default())
}

/// transform the content to text raw shortcut and custom ignore
pub fn transform_text_ignore(
    html: &str,
    custom_ignore: &Option<std::collections::HashSet<String>>,
) -> String {
    super::text_extract::extract_text(html, custom_ignore)
}

/// get the HTML content for the page (sync).
fn get_html(res: &Page, encoding: &Option<String>) -> String {
    match encoding {
        Some(ref encoding) => res.get_html_encoded(encoding),
        _ => res.get_html(),
    }
}

/// Get HTML bytes safely, handling disk-spooled pages (sync).
/// Returns Cow::Borrowed when in memory (zero cost), Cow::Owned when on disk.
#[inline]
fn get_html_bytes_safe(page: &Page) -> std::borrow::Cow<'_, [u8]> {
    let mem = page.get_html_bytes_u8();
    if !mem.is_empty() {
        return std::borrow::Cow::Borrowed(mem);
    }
    #[cfg(feature = "balance")]
    if page.is_html_on_disk() {
        return std::borrow::Cow::Owned(page.get_html().into_bytes());
    }
    std::borrow::Cow::Borrowed(mem)
}

/// get the screenshot as base64
#[cfg(feature = "screenshot")]
fn get_screenshot(res: &Page) -> String {
    use base64::{engine::general_purpose, Engine as _};
    match &res.screenshot_bytes {
        Some(content) => general_purpose::URL_SAFE.encode(&content),
        _ => Default::default(),
    }
}

/// get the screenshot as base64
#[cfg(not(feature = "screenshot"))]
fn get_screenshot(_res: &Page) -> String {
    Default::default()
}

/// get the html with the root selector
fn get_html_with_selector(
    res: &Page,
    encoding: &Option<String>,
    selector_config: &Option<SelectorConfiguration>,
) -> String {
    use scraper::{Html, Selector};
    let html = get_html(res, encoding);

    get_html_with_selector_impl(html, selector_config)
}

fn get_html_with_selector_impl(
    html: String,
    selector_config: &Option<SelectorConfiguration>,
) -> String {
    use scraper::{Html, Selector};

    if let Some(selector_config) = selector_config.as_ref() {
        let mut fragment = Html::parse_fragment(&html);

        if let Some(selector) = selector_config.root_selector.as_ref() {
            if let Ok(parsed_selector) = Selector::parse(selector) {
                if let Some(root_node) = fragment.select(&parsed_selector).next() {
                    if selector_config.exclude_selector.is_some() {
                        fragment.clone_from(&Html::parse_fragment(&root_node.html()));
                    } else {
                        // return the direct html found
                        return root_node.html();
                    }
                }
            }
        }

        if let Some(exclude_selector) = selector_config.exclude_selector.as_ref() {
            if let Ok(exclude_sel) = Selector::parse(exclude_selector) {
                let mut elements_to_remove = vec![];

                for elem in fragment.root_element().select(&exclude_sel) {
                    elements_to_remove.push(elem.id());
                }

                for id in elements_to_remove {
                    fragment.remove_node(id);
                }
            }
        }

        return fragment.root_element().html();
    }

    html
}

/// get the html with the root selector
#[inline]
fn get_html_with_selector_bytes(
    content: &[u8],
    encoding: Option<&str>,
    selector_config: Option<&SelectorConfiguration>,
) -> String {
    use scraper::{Html, Selector};

    let html = match encoding {
        Some(e) => auto_encoder::encode_bytes(content, e),
        _ => auto_encoder::auto_encode_bytes(content),
    };

    // Fast path: no selector work
    let Some(cfg) = selector_config else {
        return html;
    };

    // If both selectors are None, avoid parse entirely
    if cfg.root_selector.is_none() && cfg.exclude_selector.is_none() {
        return html;
    }

    // Parse fragment once
    let mut fragment = Html::parse_fragment(&html);

    // Root selector handling
    if let Some(selector) = cfg.root_selector.as_deref() {
        if let Ok(parsed_selector) = Selector::parse(selector) {
            if let Some(root_node) = fragment.select(&parsed_selector).next() {
                if cfg.exclude_selector.is_some() {
                    // We need to remove excluded nodes only inside the root,
                    // so re-scope fragment to the selected root.
                    // root_node.html() allocates (unavoidable).
                    fragment = Html::parse_fragment(&root_node.html());
                } else {
                    // Return direct html from selected node (allocates; required)
                    return root_node.html();
                }
            }
        }
    }

    // Exclude selector handling (collect IDs then remove; cannot mutate while iterating)
    if let Some(exclude_selector) = cfg.exclude_selector.as_deref() {
        if let Ok(exclude_sel) = Selector::parse(exclude_selector) {
            // Small optimization: avoid reallocs for common cases
            let mut ids = Vec::with_capacity(32);

            for elem in fragment.root_element().select(&exclude_sel) {
                ids.push(elem.id());
            }

            for id in ids {
                fragment.remove_node(id);
            }
        }
    }

    fragment.root_element().html()
}

/// Transform format the content.
pub fn transform_content(
    res: &Page,
    c: &TransformConfig,
    encoding: &Option<String>,
    selector_config: &Option<SelectorConfiguration>,
    ignore_tags: &Option<Vec<String>>,
) -> String {
    let base_html = get_html_with_selector(res, encoding, selector_config);

    // prevent transforming binary files or re-encoding it
    let html_bytes = get_html_bytes_safe(res);
    if is_binary_file(&*html_bytes) {
        #[cfg(feature = "document")]
        {
            if let Some(md) = crate::transformation::document::try_convert_document(&*html_bytes) {
                return md;
            }
        }
        #[cfg(feature = "audio")]
        {
            if let Some(md) = crate::transformation::audio::try_convert_audio(&*html_bytes) {
                return md;
            }
        }
        return base_html;
    }
    drop(html_bytes);

    let url_parsed = res.get_url_parsed_ref();

    let base_html = {
        let mut ignore_list = build_static_vector(c);

        if let Some(ignore) = ignore_tags {
            ignore_list.extend(ignore.iter().map(|s| s.as_str()));
        }

        if ignore_list.is_empty() {
            base_html
        } else {
            clean_html_elements(&base_html, ignore_list)
        }
    };

    // process readability
    let base_html = if c.readability {
        match llm_readability::extractor::extract(
            &mut base_html.as_bytes(),
            match url_parsed {
                Some(u) => u,
                _ => &EXAMPLE_URL,
            },
        ) {
            Ok(product) => product.content,
            _ => base_html,
        }
    } else {
        base_html
    };

    let base_html = if c.clean_html {
        clean_html(&base_html)
    } else {
        base_html
    };

    let tag_factory = ignore_tags.as_ref().map(|v| build_ignore_set(v));

    match c.return_format {
        ReturnFormat::Empty => Default::default(),
        ReturnFormat::Screenshot => get_screenshot(&res),
        ReturnFormat::Raw | ReturnFormat::Bytes => base_html,
        ReturnFormat::CommonMark => {
            html2md::rewrite_html_custom_with_url(&base_html, &tag_factory, true, url_parsed)
        }
        ReturnFormat::Markdown => {
            html2md::rewrite_html_custom_with_url(&base_html, &tag_factory, false, url_parsed)
        }
        ReturnFormat::Html2Text => {
            if !base_html.is_empty() {
                crate::html2text::from_read(base_html.as_bytes(), base_html.len())
            } else {
                base_html
            }
        }
        ReturnFormat::Text => super::text_extract::extract_text(&base_html, &tag_factory),
        ReturnFormat::XML => convert_html_to_xml(
            base_html.trim(),
            url_parsed
                .as_ref()
                .map(|u| u.as_str())
                .unwrap_or(EXAMPLE_URL.as_str()),
            encoding,
        )
        .unwrap_or_default(),
    }
}

/// Transform format the content send.
/// Transform a page's content using the byte-based pipeline.
///
/// When the page's HTML is in memory, bytes are used directly (zero-copy).
/// When on disk (`is_html_on_disk()`), content is read asynchronously via
/// `get_html_async()` — no sync blocking on tokio workers.
///
/// The actual transformation is delegated to
/// [`transform_content_send_from_url_and_bytes`] which handles selectors,
/// readability, markdown, text extraction, etc. from raw bytes.
pub async fn transform_content_send(
    res: &Page,
    c: &TransformConfig,
    encoding: &Option<String>,
    selector_config: &Option<SelectorConfiguration>,
    ignore_tags: &Option<Vec<String>>,
) -> String {
    let ignore_strs: Option<Vec<&str>> = ignore_tags
        .as_ref()
        .map(|v| v.iter().map(|s| s.as_str()).collect());

    // Fast path: content is in memory — use bytes directly, no async overhead.
    if !res.is_html_on_disk() {
        let bytes = get_html_bytes_safe(res);
        let input = TransformInput {
            url: res.get_url_parsed_ref().as_ref(),
            content: &bytes,
            screenshot_bytes: {
                #[cfg(feature = "screenshot")]
                {
                    res.screenshot_bytes.as_deref()
                }
                #[cfg(not(feature = "screenshot"))]
                {
                    None
                }
            },
            encoding: encoding.as_deref(),
            selector_config: selector_config.as_ref(),
            ignore_tags: ignore_strs.as_deref(),
        };
        return transform_content_send_from_url_and_bytes(input, c).await;
    }

    // Disk path: read content asynchronously, then feed to the byte pipeline.
    let html = res.get_html_async().await;
    let input = TransformInput {
        url: res.get_url_parsed_ref().as_ref(),
        content: html.as_bytes(),
        screenshot_bytes: {
            #[cfg(feature = "screenshot")]
            {
                res.screenshot_bytes.as_deref()
            }
            #[cfg(not(feature = "screenshot"))]
            {
                None
            }
        },
        encoding: encoding.as_deref(),
        selector_config: selector_config.as_ref(),
        ignore_tags: ignore_strs.as_deref(),
    };
    transform_content_send_from_url_and_bytes(input, c).await
}

/// Transform content input send.
pub async fn transform_content_send_from_url_and_bytes(
    input: TransformInput<'_>,
    c: &TransformConfig,
) -> String {
    use std::collections::HashSet;

    let base_html =
        get_html_with_selector_bytes(input.content, input.encoding, input.selector_config);

    // prevent transforming binary files or re-encoding it
    if is_binary_file(input.content) {
        #[cfg(feature = "document")]
        {
            if let Some(md) = crate::transformation::document::try_convert_document(input.content) {
                return md;
            }
        }
        #[cfg(feature = "audio")]
        {
            if let Some(md) = crate::transformation::audio::try_convert_audio(input.content) {
                return md;
            }
        }
        return base_html;
    }

    let base_html = {
        let mut ignore_list = build_static_vector(c);

        if let Some(ignore) = input.ignore_tags {
            ignore_list.extend(ignore.iter().copied());
        }

        if ignore_list.is_empty() {
            base_html
        } else {
            clean_html_elements(&base_html, ignore_list)
        }
    };

    // process readability
    let base_html = if c.readability {
        match llm_readability::extractor::extract(
            &mut base_html.as_bytes(),
            input.url.unwrap_or(&EXAMPLE_URL),
        ) {
            Ok(product) => product.content,
            Err(_) => base_html,
        }
    } else {
        base_html
    };

    let base_html = if c.clean_html {
        clean_html(&base_html)
    } else {
        base_html
    };

    // Build ignore set only if needed
    let tag_factory: Option<HashSet<String>> = input
        .ignore_tags
        .map(|ignore| build_ignore_set_from_strs(ignore));

    match c.return_format {
        ReturnFormat::Empty => String::new(),

        ReturnFormat::Screenshot => {
            #[cfg(feature = "screenshot")]
            {
                screenshot_base64_urlsafe(input.screenshot_bytes)
            }
            #[cfg(not(feature = "screenshot"))]
            {
                String::new()
            }
        }

        ReturnFormat::Raw | ReturnFormat::Bytes => base_html,

        ReturnFormat::CommonMark => {
            html2md::rewrite_html_custom_with_url_streaming(
                &base_html,
                &tag_factory,
                true,
                &input.url.cloned(),
            )
            .await
        }

        ReturnFormat::Markdown => {
            html2md::rewrite_html_custom_with_url_streaming(
                &base_html,
                &tag_factory,
                false,
                &input.url.cloned(),
            )
            .await
        }

        ReturnFormat::Html2Text => {
            if !base_html.is_empty() {
                crate::html2text::from_read(base_html.as_bytes(), base_html.len())
            } else {
                base_html
            }
        }

        ReturnFormat::Text => {
            super::text_extract::extract_text_streaming(&base_html, &tag_factory).await
        }

        ReturnFormat::XML => convert_html_to_xml(
            base_html.trim(),
            input
                .url
                .map(|u| u.as_str())
                .unwrap_or(EXAMPLE_URL.as_str()),
            &input.encoding.map(|s| s.to_string()),
        )
        .unwrap_or_default(),
    }
}

#[inline]
/// Transform content input.
pub fn transform_content_input(input: TransformInput<'_>, c: &TransformConfig) -> String {
    use std::collections::HashSet;

    // Build base html from raw bytes using existing logic adapted to bytes.
    let base_html =
        get_html_with_selector_bytes(input.content, input.encoding, input.selector_config);

    // prevent transforming binary files or re-encoding it
    if is_binary_file(input.content) {
        #[cfg(feature = "document")]
        {
            if let Some(md) = crate::transformation::document::try_convert_document(input.content) {
                return md;
            }
        }
        #[cfg(feature = "audio")]
        {
            if let Some(md) = crate::transformation::audio::try_convert_audio(input.content) {
                return md;
            }
        }
        return base_html;
    }

    let base_html = {
        let mut ignore_list = build_static_vector(c);

        if let Some(ignore) = input.ignore_tags {
            ignore_list.extend(ignore.iter().copied());
        }

        if ignore_list.is_empty() {
            base_html
        } else {
            clean_html_elements(&base_html, ignore_list)
        }
    };

    // process readability
    let base_html = if c.readability {
        match llm_readability::extractor::extract(
            &mut base_html.as_bytes(),
            input.url.unwrap_or(&EXAMPLE_URL),
        ) {
            Ok(product) => product.content,
            Err(_) => base_html,
        }
    } else {
        base_html
    };

    let base_html = if c.clean_html {
        clean_html(&base_html)
    } else {
        base_html
    };

    // Build ignore tag set only if needed by downstream (md/text extract).
    let tag_factory: Option<HashSet<String>> = input
        .ignore_tags
        .map(|ignore| build_ignore_set_from_strs(ignore));

    match c.return_format {
        ReturnFormat::Empty => String::new(),

        ReturnFormat::Screenshot => {
            #[cfg(feature = "screenshot")]
            {
                screenshot_base64_urlsafe(input.screenshot_bytes)
            }
            #[cfg(not(feature = "screenshot"))]
            {
                String::new()
            }
        }

        ReturnFormat::Raw | ReturnFormat::Bytes => base_html,

        ReturnFormat::CommonMark => html2md::rewrite_html_custom_with_url(
            &base_html,
            &tag_factory,
            true,
            &input.url.cloned(),
        ),
        ReturnFormat::Markdown => html2md::rewrite_html_custom_with_url(
            &base_html,
            &tag_factory,
            false,
            &input.url.cloned(),
        ),

        ReturnFormat::Html2Text => {
            if !base_html.is_empty() {
                crate::html2text::from_read(base_html.as_bytes(), base_html.len())
            } else {
                base_html
            }
        }

        ReturnFormat::Text => super::text_extract::extract_text(&base_html, &tag_factory),

        ReturnFormat::XML => convert_html_to_xml(
            base_html.trim(),
            input
                .url
                .map(|u| u.as_str())
                .unwrap_or(EXAMPLE_URL.as_str()),
            &input.encoding.map(|s| s.to_string()),
        )
        .unwrap_or_default(),
    }
}

#[cfg(feature = "screenshot")]
#[inline]
fn screenshot_base64_urlsafe(screenshot_bytes: Option<&[u8]>) -> String {
    use base64::{engine::general_purpose, Engine as _};

    let Some(bytes) = screenshot_bytes else {
        return String::new();
    };

    // Single allocation: reserve exact output length.
    let cap = base64::encoded_len(bytes.len(), true).unwrap_or(0);
    let mut out = String::with_capacity(cap);
    general_purpose::URL_SAFE.encode_string(bytes, &mut out);
    out
}

/// transform the content to bytes to prevent loss of precision.
pub fn transform_content_to_bytes(
    res: &Page,
    c: &TransformConfig,
    encoding: &Option<String>,
    selector_config: &Option<SelectorConfiguration>,
    ignore_tags: &Option<Vec<String>>,
) -> Vec<u8> {
    let html_raw = get_html_bytes_safe(res);
    if is_binary_file(&html_raw) {
        #[cfg(feature = "document")]
        {
            if let Some(md) = crate::transformation::document::try_convert_document(&html_raw) {
                return md.into_bytes();
            }
        }
        #[cfg(feature = "audio")]
        {
            if let Some(md) = crate::transformation::audio::try_convert_audio(&html_raw) {
                return md.into_bytes();
            }
        }
        let b = res.get_bytes();
        if let Some(b) = b {
            b.to_vec()
        } else {
            Default::default()
        }
    } else {
        transform_content(res, c, encoding, selector_config, ignore_tags).into()
    }
}
