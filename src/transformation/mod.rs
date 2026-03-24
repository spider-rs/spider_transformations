/// Chunking utils.
pub mod chunking;
/// Content utils.
pub mod content;
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
        assert!(result.contains("Hello world"), "should extract text: {result}");
        assert!(!result.contains("var x"), "should exclude script content: {result}");
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
        let html = r#"<div><nav>nav text</nav><footer>footer text</footer><main>main text</main></div>"#;
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

        let result =
            super::text_extract::extract_text_streaming(html, &Some(ignore)).await;
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
        let result =
            super::text_extract::extract_text_streaming(html, &None).await;
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

        let result =
            content::transform_content(&page, &conf, &None, &None, &ignore_tags);

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

        let result = content::transform_content(
            &page,
            &conf,
            &None,
            &Some(select_config),
            &ignore_tags,
        );

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
}
