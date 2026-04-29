/// Audio file metadata extraction.
#[cfg(feature = "audio")]
pub mod audio;
/// Chunking utils.
pub mod chunking;
/// Content utils.
pub mod content;
/// Office document (xlsx, docx, pptx) conversion.
#[cfg(feature = "document")]
pub mod document;
/// Text extraction.
pub mod text_extract;

#[cfg(test)]
mod tests {
    use std::vec;

    use crate::transformation::content::{self, ReturnFormat, SelectorConfiguration};
    use maud::PreEscaped;
    use spider::{
        page::build_with_parse,
        tokio::{self, fs::File},
        utils::PageResponse,
    };

    /// the template to re-use
    fn template() -> PreEscaped<String> {
        use maud::{html, DOCTYPE};

        let page_title = "Transform Test";
        let page_h1 = "Fun is fun";

        let markup = html! {
            (DOCTYPE)
            meta charset="utf-8";
            title { (page_title) }
            h1 { (page_h1) }
            a href="https://spider.cloud" { "Spider Cloud"};
            pre {
                r#"The content is ready"#
            }
            script {
                r#"document.querySelector("pre")"#
            }
        };

        markup
    }

    #[test]
    fn test_transformations() {
        let markup = template().into_string();
        let url = "https://spider.cloud";

        let mut conf = content::TransformConfig::default();
        let mut page_response = PageResponse::default();

        page_response.content = Some(markup.into());
        let page = build_with_parse(url, page_response);

        conf.return_format = ReturnFormat::Markdown;

        let content = content::transform_content(&page, &conf, &None, &None, &None);

        assert!(
            content
                .contains(&"Transform Test# Fun is fun\n[Spider Cloud](https://spider.cloud)\n```\nThe content is ready\n```"),
            "The tranform to markdown is invalid"
        );

        conf.return_format = ReturnFormat::Html2Text;

        let content = content::transform_content(&page, &conf, &None, &None, &None);

        assert!(
            content
                .contains(& "# Fun is fun\n\n[Spider Cloud][1]\nThe content is ready\n\n[1]: https://spider.cloud\n"),
            "The tranform to html2text is invalid"
        );

        conf.return_format = ReturnFormat::Bytes;
        conf.readability = true;

        let content = content::transform_content(&page, &conf, &None, &None, &None);

        assert!(
            content
                .contains(&"<html class=\"paper\"><head>\n<meta name=\"disabled-adaptations\" content=\"watch\">\n<meta http-equiv=\"Content-Type\" content=\"text/html; charset=utf-8\">\n<meta name=\"viewport\" content=\"initial-scale=1\">\n<base href=\"https://spider.cloud/\">\n<title>Transform Test</title>\n<script>window.isReaderPage = true;</script>\n</head><body>\n<h1>Fun is fun</h1><a href=\"https://spider.cloud\">Spider Cloud</a><pre>The content is ready</pre></body></html>"),
            "The tranform to bytes is invalid"
        );

        conf.return_format = ReturnFormat::XML;
        let content = content::transform_content(&page, &conf, &Some("UTF-8".into()), &None, &None);
        assert!(
            content
                == r#"<html xmlns="http://www.w3.org/1999/xhtml" class="paper"><head>
<meta name="disabled-adaptations" content="watch" />
<meta http-equiv="Content-Type" content="text/html; charset=utf-8" />
<meta name="viewport" content="initial-scale=1" />
<base href="https://spider.cloud/" />
<title>Transform Test</title>
<script><![CDATA[window.isReaderPage = true;]]></script>
</head><body>
<h1>Fun is fun</h1><a href="https://spider.cloud">Spider Cloud</a><pre>The content is ready</pre></body></html>"#,
            "The tranform to xml is invalid"
        );
    }

    #[test]
    fn test_xml_transformations() {
        let markup = template().into_string();

        let url = "https://spider.cloud";

        let mut conf = content::TransformConfig::default();
        let mut page_response = PageResponse::default();
        conf.return_format = ReturnFormat::XML;
        page_response.content = Some(markup.into());
        let page = build_with_parse(url, page_response);
        let content = content::transform_content(&page, &conf, &None, &None, &None);
        assert!(
            content
                == r#"<!DOCTYPE html><html xmlns="http://www.w3.org/1999/xhtml"><head><meta charset="utf-8" /><title>Transform Test</title></head><body><h1>Fun is fun</h1><a href="https://spider.cloud">Spider Cloud</a><pre>The content is ready</pre><script><![CDATA[document.querySelector(&amp;quot;pre&amp;quot;)]]></script></body></html>"#,
            "The tranform to xml is invalid"
        );
    }

    #[test]
    fn test_transformations_root_selector() {
        let markup = template().into_string();
        let url = "https://spider.cloud";

        let mut conf = content::TransformConfig::default();
        let mut page_response = PageResponse::default();

        page_response.content = Some(markup.into());
        let page = build_with_parse(url, page_response);

        conf.return_format = ReturnFormat::Markdown;

        let mut select_config = SelectorConfiguration::default();

        select_config.root_selector = Some("pre".into());

        let content = content::transform_content(&page, &conf, &None, &Some(select_config), &None);

        assert!(
            content.contains(&"The content is ready"),
            "The tranform to markdown is invalid"
        );
    }

    #[test]
    fn test_transformations_exclude_selector() {
        let markup = template().into_string();
        let url = "https://spider.cloud";

        let mut conf = content::TransformConfig::default();
        let mut page_response = PageResponse::default();

        page_response.content = Some(markup.into());
        let page = build_with_parse(url, page_response);

        conf.return_format = ReturnFormat::Markdown;

        let mut select_config = SelectorConfiguration::default();

        select_config.exclude_selector = Some("pre".into());

        let content = content::transform_content(&page, &conf, &None, &Some(select_config), &None);

        assert!(
            content.contains(&"Transform Test# Fun is fun\n[Spider Cloud](https://spider.cloud)"),
            "The tranform to markdown is invalid"
        );
    }

    #[test]
    fn test_transformations_exclude_selector_text() {
        let markup = template().into_string();
        let url = "https://spider.cloud";

        let mut conf = content::TransformConfig::default();
        let mut page_response = PageResponse::default();

        page_response.content = Some(markup.into());
        let page = build_with_parse(url, page_response);

        conf.return_format = ReturnFormat::Text;

        let mut select_config = SelectorConfiguration::default();

        select_config.exclude_selector = Some("pre".into());

        let content = content::transform_content(&page, &conf, &None, &Some(select_config), &None);

        assert!(
            content.contains(&"Transform Test\nFun is fun Spider Cloud"),
            "The tranform to markdown is invalid"
        );
    }

    #[tokio::test]
    async fn test_transformations_exclude_selector_text_streaming() {
        let markup = template().into_string();
        let url = "https://spider.cloud";

        let mut conf = content::TransformConfig::default();
        let mut page_response = PageResponse::default();

        page_response.content = Some(markup.into());
        let page = build_with_parse(url, page_response);

        conf.return_format = ReturnFormat::Text;

        let mut select_config = SelectorConfiguration::default();

        select_config.exclude_selector = Some("pre".into());

        let content =
            content::transform_content_send(&page, &conf, &None, &Some(select_config), &None).await;

        assert!(
            content.contains(&"Transform Test\nFun is fun Spider Cloud"),
            "The tranform to markdown is invalid"
        );
    }

    #[ignore]
    #[tokio::test]
    async fn test_transformations_pdf_handling() {
        use spider::tokio::io::AsyncReadExt;
        let mut f = File::open("./example.pdf").await.unwrap();
        let mut data = vec![];
        f.read_to_end(&mut data).await.unwrap();

        let mut conf = content::TransformConfig::default();
        conf.return_format = ReturnFormat::XML;
        let mut page_response = PageResponse::default();

        page_response.content = Some(data);

        let page = build_with_parse("https://example.com/example.pdf", page_response);

        let content = content::transform_content(&page, &conf, &None, &None, &None);

        assert!(content.is_empty(), "The tranform to markdown is invalid");
    }

    // -------------------------------------------------------------------
    // extract_text: two-pass fix tests
    // -------------------------------------------------------------------

    #[test]
    fn test_extract_text_no_custom_ignore() {
        let html = r#"<div><p>Hello world</p><script>var x=1;</script></div>"#;
        let result = super::text_extract::extract_text(html, &None);
        assert!(
            result.contains("Hello world"),
            "should extract text: {result}"
        );
        assert!(
            !result.contains("var x"),
            "should exclude script content: {result}"
        );
    }

    #[test]
    fn test_extract_text_custom_ignore_strips_nested_element() {
        // The bug: custom ignore tags via element! remove() didn't prevent
        // text handlers from capturing the removed element's text when nested.
        let html = r#"<div><div class="popup">popup text</div><p>real content</p></div>"#;
        let mut ignore = std::collections::HashSet::new();
        ignore.insert(".popup".to_string());

        let result = super::text_extract::extract_text(html, &Some(ignore));
        assert!(
            !result.contains("popup text"),
            "custom ignore should strip nested .popup text, got: {result}"
        );
        assert!(
            result.contains("real content"),
            "should preserve non-ignored text, got: {result}"
        );
    }

    #[test]
    fn test_extract_text_custom_ignore_strips_deeply_nested() {
        let html = r#"<body><div class="wrapper"><aside class="sidebar"><nav><ul><li>Link 1</li><li>Link 2</li></ul></nav></aside><main><p>Article content</p></main></div></body>"#;
        let mut ignore = std::collections::HashSet::new();
        ignore.insert(".sidebar".to_string());

        let result = super::text_extract::extract_text(html, &Some(ignore));
        assert!(
            !result.contains("Link 1"),
            "sidebar text should be stripped, got: {result}"
        );
        assert!(
            result.contains("Article content"),
            "main content preserved, got: {result}"
        );
    }

    #[test]
    fn test_extract_text_custom_ignore_multiple_selectors() {
        let html =
            r#"<div><nav>nav text</nav><footer>footer text</footer><main>main text</main></div>"#;
        let mut ignore = std::collections::HashSet::new();
        ignore.insert("nav".to_string());
        ignore.insert("footer".to_string());

        let result = super::text_extract::extract_text(html, &Some(ignore));
        assert!(
            !result.contains("nav text"),
            "nav should be stripped, got: {result}"
        );
        assert!(
            !result.contains("footer text"),
            "footer should be stripped, got: {result}"
        );
        assert!(
            result.contains("main text"),
            "main content preserved, got: {result}"
        );
    }

    #[test]
    fn test_extract_text_empty_custom_ignore() {
        let html = r#"<div><p>content</p></div>"#;
        let ignore = std::collections::HashSet::new();
        let result = super::text_extract::extract_text(html, &Some(ignore));
        assert!(
            result.contains("content"),
            "empty ignore set should not affect extraction, got: {result}"
        );
    }

    #[test]
    fn test_extract_text_empty_html() {
        let result = super::text_extract::extract_text("", &None);
        assert!(result.is_empty(), "empty html should produce empty text");
    }

    #[test]
    fn test_extract_text_script_style_svg_excluded() {
        let html = r#"<div><p>visible</p><script>js code</script><style>.x{}</style><svg><text>svg text</text></svg><noscript>noscript</noscript></div>"#;
        let result = super::text_extract::extract_text(html, &None);
        assert!(result.contains("visible"), "should extract visible text");
        assert!(!result.contains("js code"), "should exclude script");
        assert!(!result.contains(".x{}"), "should exclude style");
    }

    #[tokio::test]
    async fn test_extract_text_streaming_custom_ignore() {
        let html = r#"<div><div class="popup">popup text</div><p>real content</p></div>"#;
        let mut ignore = std::collections::HashSet::new();
        ignore.insert(".popup".to_string());

        let result = super::text_extract::extract_text_streaming(html, &Some(ignore)).await;
        assert!(
            !result.contains("popup text"),
            "streaming: custom ignore should strip .popup text, got: {result}"
        );
        assert!(
            result.contains("real content"),
            "streaming: should preserve non-ignored text, got: {result}"
        );
    }

    #[tokio::test]
    async fn test_extract_text_streaming_no_ignore() {
        let html = r#"<div><p>hello</p><script>bad</script></div>"#;
        let result = super::text_extract::extract_text_streaming(html, &None).await;
        assert!(result.contains("hello"), "streaming: should extract text");
        assert!(!result.contains("bad"), "streaming: should exclude script");
    }

    // Regression: ensure the existing exclude_selector + Text format pipeline
    // still works correctly end-to-end.
    #[test]
    fn test_transform_content_text_with_exclude_and_ignore() {
        use maud::{html, DOCTYPE};

        let markup = html! {
            (DOCTYPE)
            title { "Test Page" }
            nav { "Navigation Menu" }
            div class="sidebar" { "Sidebar Widget" }
            main {
                h1 { "Article" }
                p { "Important content here." }
            }
            footer { "Footer Links" }
        };

        let url = "https://example.com";
        let mut conf = content::TransformConfig::default();
        let mut page_response = spider::utils::PageResponse::default();
        page_response.content = Some(markup.into_string().into());
        let page = build_with_parse(url, page_response);

        conf.return_format = ReturnFormat::Text;

        // With ignore_tags that should strip nav and footer
        let ignore_tags = Some(vec!["nav".to_string(), "footer".to_string()]);

        let result = content::transform_content(&page, &conf, &None, &None, &ignore_tags);

        assert!(
            result.contains("Important content"),
            "main content should be present, got: {result}"
        );
        assert!(
            !result.contains("Navigation Menu"),
            "nav should be stripped by ignore_tags, got: {result}"
        );
        assert!(
            !result.contains("Footer Links"),
            "footer should be stripped by ignore_tags, got: {result}"
        );
    }

    #[test]
    fn test_transform_content_text_with_selector_and_ignore() {
        use maud::{html, DOCTYPE};

        let markup = html! {
            (DOCTYPE)
            title { "Test" }
            header { "Header" }
            main {
                div class="ad-banner" { "Ad content" }
                p { "Real article text" }
            }
            footer { "Footer" }
        };

        let url = "https://example.com";
        let mut conf = content::TransformConfig::default();
        let mut page_response = spider::utils::PageResponse::default();
        page_response.content = Some(markup.into_string().into());
        let page = build_with_parse(url, page_response);

        conf.return_format = ReturnFormat::Text;

        // exclude_selector removes header/footer, ignore_tags removes .ad-banner
        let mut select_config = SelectorConfiguration::default();
        select_config.exclude_selector = Some("header, footer".into());
        let ignore_tags = Some(vec![".ad-banner".to_string()]);

        let result =
            content::transform_content(&page, &conf, &None, &Some(select_config), &ignore_tags);

        assert!(
            result.contains("Real article text"),
            "main content should be present, got: {result}"
        );
        assert!(
            !result.contains("Header"),
            "header should be excluded, got: {result}"
        );
        assert!(
            !result.contains("Footer"),
            "footer should be excluded, got: {result}"
        );
        assert!(
            !result.contains("Ad content"),
            "ad-banner should be stripped by ignore_tags, got: {result}"
        );
    }

    // ===================================================================
    // Panic-freedom / lock-free audit tests
    // ===================================================================

    /// Verify html2text handles tables without panicking (previously used
    /// `unimplemented!()` for TableRow/TableBody/TableCell size estimation).
    #[test]
    fn no_panic_html2text_with_tables() {
        let html = r#"<table><thead><tr><th>A</th><th>B</th></tr></thead>
            <tbody><tr><td>1</td><td>2</td></tr>
            <tr><td>3</td><td>4</td></tr></tbody></table>"#;
        let result = crate::html2text::from_read(html.as_bytes(), 80);
        assert!(!result.is_empty(), "table rendering should produce output");
    }

    /// Nested tables should not panic.
    #[test]
    fn no_panic_html2text_nested_tables() {
        let html = r#"<table><tr><td><table><tr><td>inner</td></tr></table></td></tr></table>"#;
        let result = crate::html2text::from_read(html.as_bytes(), 80);
        assert!(
            result.contains("inner"),
            "nested table content should render: {result}"
        );
    }

    /// Very wide table with many columns should not panic.
    #[test]
    fn no_panic_html2text_wide_table() {
        let mut html = String::from("<table><tr>");
        for i in 0..100 {
            html.push_str(&format!("<td>col{i}</td>"));
        }
        html.push_str("</tr></table>");
        let result = crate::html2text::from_read(html.as_bytes(), 40);
        // Should not panic even with narrow width and many columns.
        assert!(!result.is_empty());
    }

    /// html2text with zero-width should not panic.
    #[test]
    fn no_panic_html2text_zero_width() {
        let html = "<p>Hello world</p>";
        // width=0 might return an error or empty, but must not panic
        let result = crate::html2text::from_read(html.as_bytes(), 0);
        let _ = result; // just verifying no panic
    }

    /// html2text with empty input should not panic.
    #[test]
    fn no_panic_html2text_empty() {
        let result = crate::html2text::from_read("".as_bytes(), 80);
        assert!(result.is_empty() || result.trim().is_empty());
    }

    /// Chunk by sentence with zero chunk size must not panic.
    /// Previously `Vec::chunks(0)` would panic.
    #[test]
    fn no_panic_chunk_by_sentence_zero() {
        use crate::transformation::chunking::{chunk_text, ChunkingAlgorithm};
        let result = chunk_text(
            "Hello world. How are you? Fine.",
            ChunkingAlgorithm::BySentence(0),
        );
        assert!(!result.is_empty(), "should produce at least one chunk");
    }

    /// Chunk by words with zero chunk size must not panic.
    #[test]
    fn no_panic_chunk_by_words_zero() {
        use crate::transformation::chunking::{chunk_text, ChunkingAlgorithm};
        let result = chunk_text("Hello world foo bar", ChunkingAlgorithm::ByWords(0));
        // With 0, every word becomes its own chunk (>= 0 is always true)
        assert!(!result.is_empty());
    }

    /// Chunk by lines with zero must not panic.
    #[test]
    fn no_panic_chunk_by_lines_zero() {
        use crate::transformation::chunking::{chunk_text, ChunkingAlgorithm};
        let result = chunk_text("line1\nline2\nline3", ChunkingAlgorithm::ByLines(0));
        assert!(!result.is_empty());
    }

    /// Chunk by char length with zero must not panic.
    #[test]
    fn no_panic_chunk_by_char_zero() {
        use crate::transformation::chunking::{chunk_text, ChunkingAlgorithm};
        let result = chunk_text("abcdef", ChunkingAlgorithm::ByCharacterLength(0));
        assert!(!result.is_empty());
    }

    /// Chunking on empty text must not panic for any algorithm.
    #[test]
    fn no_panic_chunk_empty_text() {
        use crate::transformation::chunking::{chunk_text, ChunkingAlgorithm};
        assert!(chunk_text("", ChunkingAlgorithm::ByWords(5)).is_empty());
        assert!(chunk_text("", ChunkingAlgorithm::ByLines(5)).is_empty());
        assert!(chunk_text("", ChunkingAlgorithm::ByCharacterLength(5)).is_empty());
        // BySentence splits on regex, so empty string still produces one empty split
        let _ = chunk_text("", ChunkingAlgorithm::BySentence(5));
    }

    /// extract_text with malformed/truncated HTML must not panic.
    #[test]
    fn no_panic_extract_text_malformed_html() {
        let cases = [
            "<div><p>unclosed",
            "</p></div>stray closing",
            "<sc<ript>broken tag",
            "<<<>>>",
            "<div attr=\"unclosed>text</div>",
            &"<div>".repeat(1000),
            "",
        ];
        for html in &cases {
            let _ = super::text_extract::extract_text(html, &None);
        }
    }

    /// Streaming extract_text with malformed HTML must not panic.
    #[tokio::test]
    async fn no_panic_extract_text_streaming_malformed() {
        let cases = ["<div><p>unclosed", "</p></div>stray", "<<<>>>", ""];
        for html in &cases {
            let _ = super::text_extract::extract_text_streaming(html, &None).await;
        }
    }

    /// Parity: when the chunk size is at least as large as the input,
    /// streaming feeds the rewriter in a single write — that is the same
    /// success-path that the deadlock fix now scopes. Output must be
    /// byte-identical to the sync `extract_text` reference. This pins the
    /// success-path behavior so the scoping change (rewriter dropping at
    /// scope end vs function end) cannot silently regress output for any
    /// shape of input.
    #[tokio::test]
    async fn extract_text_streaming_matches_sync_reference_single_write() {
        let small = "<p>hello <b>world</b></p>".to_string();
        let mixed =
            "<div><p>real</p><script>bad</script><style>.x{}</style><p>after</p></div>".to_string();
        let with_rcdata = "<title>page title</title><p>body text</p>".to_string();
        let nested = "<div><div><div><p>deep</p></div></div></div>".to_string();
        let unicode = "<p>こんにちは 世界 🌍</p><p>café résumé</p>".to_string();
        let entities = "<p>a &amp; b &lt;c&gt; &quot;d&quot;</p>".to_string();
        // Note: streaming has a documented 1024-byte buffer flush
        // (text_extract.rs `if extracted_text.len() > 1024 { ... clear() }`)
        // that the sync path doesn't, so direct sync==streaming parity only
        // holds for inputs that stay under that threshold. Larger inputs are
        // exercised by `extract_text_streaming_stable_across_large_chunk_sizes`
        // below as a streaming-vs-streaming invariant instead.
        let inputs: Vec<String> = vec![small, mixed, with_rcdata, nested, unicode, entities];
        for input in &inputs {
            assert!(
                input.len() < 1024,
                "parity input must stay under 1024-byte flush boundary"
            );
        }

        for input in &inputs {
            let sync_out = super::text_extract::extract_text(input, &None);
            // Force single-write: chunk_size >= input.len() means lol_html
            // sees one contiguous buffer and emits text!() callbacks once
            // per text node, identical to the sync rewrite.
            let cs = input.len() + 1;
            let stream_out =
                super::text_extract::extract_text_streaming_with_size(input, &None, cs).await;
            assert_eq!(
                stream_out,
                sync_out,
                "streaming(single-write) diverged from sync, input_len={}",
                input.len()
            );
        }

        // Custom-ignore parity (depth-counter + end-tag-handler path).
        let mut ignore = std::collections::HashSet::new();
        ignore.insert(".popup".to_string());
        let ignore = Some(ignore);
        let ignore_inputs = [
            r#"<div><div class="popup">hide</div><p>show</p></div>"#.to_string(),
            r#"<p>before</p><div class="popup"><p>nested hide</p></div><p>after</p>"#.to_string(),
            // stay under the 1024-byte streaming flush boundary
            (r#"<div class="popup">x</div>"#.to_string() + &"<p>kept</p>".repeat(50)),
        ];
        for input in &ignore_inputs {
            assert!(
                input.len() < 1024,
                "parity ignore-input must stay under 1024-byte flush boundary"
            );
        }
        for input in &ignore_inputs {
            let sync_out = super::text_extract::extract_text(input, &ignore);
            let cs = input.len() + 1;
            let stream_out =
                super::text_extract::extract_text_streaming_with_size(input, &ignore, cs).await;
            assert_eq!(
                stream_out,
                sync_out,
                "streaming(ignore, single-write) diverged from sync, input_len={}",
                input.len()
            );
        }
    }

    /// Stability: streaming output must be invariant across chunk sizes that
    /// are coarse enough to never split a single text node. We verify this
    /// for inputs where every text node fits in 64 bytes by comparing
    /// chunk_size=4096 / 8192 / 65536 — all should produce the same string
    /// as the single-write reference. This locks the public-API behavior
    /// (default chunk_size=8192) against the deadlock-fix diff.
    #[tokio::test]
    async fn extract_text_streaming_stable_across_large_chunk_sizes() {
        let inputs: Vec<String> = vec![
            "<p>alpha</p><p>beta</p><p>gamma</p>".to_string(),
            (0..200).map(|i| format!("<p>n{i}</p>")).collect::<String>(),
            "<title>t</title>".to_string() + &"<p>body</p>".repeat(500),
        ];
        for input in &inputs {
            let reference = super::text_extract::extract_text_streaming_with_size(
                input,
                &None,
                input.len() + 1,
            )
            .await;
            for cs in [4096usize, 8192, 65_536] {
                let out =
                    super::text_extract::extract_text_streaming_with_size(input, &None, cs).await;
                assert_eq!(
                    out,
                    reference,
                    "streaming output drifted at chunk_size={cs}, input_len={}",
                    input.len()
                );
            }
            // Also pin the public API default path.
            let public = super::text_extract::extract_text_streaming(input, &None).await;
            assert_eq!(public, reference, "extract_text_streaming default diverged");
        }
    }

    /// Regression: streaming extract_text must not deadlock waiting on the
    /// internal mpsc channel — if the rewriter short-circuits on error, the
    /// senders need to drop before the recv loop. Wrap calls in a tight
    /// timeout and span small/large/malformed/empty inputs.
    #[tokio::test]
    async fn extract_text_streaming_never_deadlocks() {
        use std::time::Duration;
        use tokio::time::timeout;

        let big =
            "<div>".repeat(50_000) + &"hello world ".repeat(10_000) + &"</div>".repeat(50_000);
        let cases: Vec<&str> = vec![
            "",
            "plain text",
            "<p>hi</p>",
            "<div><p>unclosed",
            "</p></div>stray",
            "<<<>>>",
            "<script>bad</script><p>good</p>",
            &big,
        ];
        for html in cases {
            let res = timeout(
                Duration::from_secs(10),
                super::text_extract::extract_text_streaming(html, &None),
            )
            .await;
            assert!(
                res.is_ok(),
                "extract_text_streaming hung on input len {}",
                html.len()
            );
        }
    }

    /// All ReturnFormat variants on empty content must not panic.
    #[test]
    fn no_panic_transform_all_formats_empty() {
        let url = "https://example.com";
        let mut page_response = PageResponse::default();
        page_response.content = Some(Vec::new());
        let page = build_with_parse(url, page_response);

        let formats = [
            ReturnFormat::Raw,
            ReturnFormat::Text,
            ReturnFormat::Markdown,
            ReturnFormat::CommonMark,
            ReturnFormat::Html2Text,
            ReturnFormat::XML,
            ReturnFormat::Bytes,
            ReturnFormat::Empty,
        ];
        for fmt in &formats {
            let mut conf = content::TransformConfig::default();
            conf.return_format = *fmt;
            let _ = content::transform_content(&page, &conf, &None, &None, &None);
        }
    }

    /// All ReturnFormat variants on valid HTML must not panic.
    #[test]
    fn no_panic_transform_all_formats_valid() {
        let markup = template().into_string();
        let url = "https://spider.cloud";
        let mut page_response = PageResponse::default();
        page_response.content = Some(markup.into());
        let page = build_with_parse(url, page_response);

        let formats = [
            ReturnFormat::Raw,
            ReturnFormat::Text,
            ReturnFormat::Markdown,
            ReturnFormat::CommonMark,
            ReturnFormat::Html2Text,
            ReturnFormat::XML,
            ReturnFormat::Bytes,
            ReturnFormat::Empty,
        ];
        for fmt in &formats {
            let mut conf = content::TransformConfig::default();
            conf.return_format = *fmt;
            let _ = content::transform_content(&page, &conf, &None, &None, &None);
        }
    }

    /// transform_content with readability on garbage HTML must not panic.
    #[test]
    fn no_panic_transform_readability_garbage() {
        let html = "<html><body><div>some text</div><script>x</script></body></html>";
        let url = "https://example.com";
        let mut page_response = PageResponse::default();
        page_response.content = Some(html.into());
        let page = build_with_parse(url, page_response);

        let mut conf = content::TransformConfig::default();
        conf.readability = true;
        conf.return_format = ReturnFormat::Markdown;
        let _ = content::transform_content(&page, &conf, &None, &None, &None);
    }

    /// XML conversion of non-UTF8-declared content must not panic.
    #[test]
    fn no_panic_xml_conversion_various_encodings() {
        let html = "<html><head><meta charset='utf-8'></head><body>Hello</body></html>";
        let url = "https://example.com";
        let mut page_response = PageResponse::default();
        page_response.content = Some(html.into());
        let page = build_with_parse(url, page_response);

        let mut conf = content::TransformConfig::default();
        conf.return_format = ReturnFormat::XML;

        // With explicit encoding
        let _ = content::transform_content(&page, &conf, &Some("UTF-8".into()), &None, &None);
        // Without encoding
        let _ = content::transform_content(&page, &conf, &None, &None, &None);
        // With unknown encoding
        let _ =
            content::transform_content(&page, &conf, &Some("FAKE-ENCODING".into()), &None, &None);
    }

    /// Large HTML document should not stack overflow or panic.
    #[test]
    fn no_panic_large_html() {
        let mut html = String::with_capacity(100_000);
        html.push_str("<html><body>");
        for i in 0..1000 {
            html.push_str(&format!(
                "<p>Paragraph {i} with <b>bold</b> and <a href='#'>link</a></p>"
            ));
        }
        html.push_str("</body></html>");

        let result = crate::html2text::from_read(html.as_bytes(), 80);
        assert!(!result.is_empty());

        let _ = super::text_extract::extract_text(&html, &None);
    }

    /// Deeply nested HTML should not stack overflow.
    #[test]
    fn no_panic_deeply_nested_html() {
        let depth = 200;
        let mut html = String::new();
        for _ in 0..depth {
            html.push_str("<div>");
        }
        html.push_str("deep content");
        for _ in 0..depth {
            html.push_str("</div>");
        }
        let _ = crate::html2text::from_read(html.as_bytes(), 80);
        let _ = super::text_extract::extract_text(&html, &None);
    }

    /// Async streaming variants must not panic on any format.
    #[tokio::test]
    async fn no_panic_transform_send_all_formats() {
        let markup = template().into_string();
        let url = "https://spider.cloud";
        let mut page_response = PageResponse::default();
        page_response.content = Some(markup.into());
        let page = build_with_parse(url, page_response);

        let formats = [
            ReturnFormat::Raw,
            ReturnFormat::Text,
            ReturnFormat::Markdown,
            ReturnFormat::CommonMark,
            ReturnFormat::Html2Text,
            ReturnFormat::XML,
            ReturnFormat::Bytes,
            ReturnFormat::Empty,
        ];
        for fmt in formats {
            let mut conf = content::TransformConfig::default();
            conf.return_format = fmt;
            let _ = content::transform_content_send(&page, &conf, &None, &None, &None).await;
        }
    }

    /// Unicode / multibyte content must not panic in any path.
    #[test]
    fn no_panic_unicode_content() {
        let html =
            "<html><body><p>日本語テスト</p><p>Ñoño café résumé</p><p>🦀🔥💯</p></body></html>";
        let _ = crate::html2text::from_read(html.as_bytes(), 80);
        let _ = super::text_extract::extract_text(html, &None);

        let url = "https://example.com";
        let mut page_response = PageResponse::default();
        page_response.content = Some(html.into());
        let page = build_with_parse(url, page_response);
        let mut conf = content::TransformConfig::default();
        for fmt in [
            ReturnFormat::Text,
            ReturnFormat::Markdown,
            ReturnFormat::Html2Text,
        ] {
            conf.return_format = fmt;
            let _ = content::transform_content(&page, &conf, &None, &None, &None);
        }
    }
}
