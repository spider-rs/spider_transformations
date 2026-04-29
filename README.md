# spider_transformations

A high-performance transformation library for Rust, used by [Spider Cloud](https://spider.cloud) for AI-powered content cleaning across multiple locales.

This project depends on the `spider` crate.

## Usage

```toml
[dependencies]
spider_transformations = "2"
```

```rust
use spider_transformations::transformation::content;

fn main() {
    // page comes from the spider object when streaming.
    let mut conf = content::TransformConfig::default();
    conf.return_format = content::ReturnFormat::Markdown;
    let content = content::transform_content(&page, &conf, &None, &None, &None);
}
```

### Transform types

1. Markdown
1. Commonmark
1. Text
1. Markdown (Text Map) or HTML2Text
1. HTML2XML

### Document formats (feature: `document`)

Convert Office documents directly to markdown:

1. Excel (.xlsx)
1. Word (.docx)
1. PowerPoint (.pptx)

Enable with:

```toml
[dependencies]
spider_transformations = { version = "2", features = ["document"] }
```

Document conversion is automatic — binary files matching Office formats are detected and converted to markdown tables and text. No configuration needed beyond enabling the feature.

#### Enhancements

1. Readability
1. Encoding

## Chunking

There are several chunking utils in the transformation mod.

This project has rewrites and forks of html2md, and html2text for performance and bug fixes.

## License

MIT
