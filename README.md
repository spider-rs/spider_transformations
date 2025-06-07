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
    let content = content::transform_content(&page, &conf, &None, &None);
}
```

### Transform types

1. Markdown
1. Commonmark
1. Text
1. Markdown (Text Map) or HTML2Text
1. WIP: HTML2XML

#### Enhancements

1. Readability
1. Encoding

## Chunking

There are several chunking utils in the transformation mod.

This project has rewrites and forks of html2md, and html2text for performance and bug fixes.

## License

MIT