use super::markup5ever_rcdom::{Handle, NodeData, RcDom};
use html5ever::tendril::TendrilSink;
use html5ever::{parse_document, QualName};
use markup5ever::namespace_url;
use markup5ever::ns;
use spider::auto_encoder::auto_encode_bytes;
use spider::page::get_html_encoded;
use std::default::Default;
use std::error::Error;
use std::io::{self, Write};

/// Convert HTML to well-formed XML.
pub fn convert_html_to_xml(
    html: &str,
    url: &str,
    encoding: &Option<String>,
) -> Result<String, Box<dyn Error>> {
    if encoding.is_some() {
        let bytes: Box<[u8]> = base_convert_xml(html, url, encoding)?.into_boxed_slice();

        Ok(get_html_encoded(
            &Some(bytes.into()),
            match encoding {
                Some(encoding) => encoding,
                _ => "UTF-8",
            },
        ))
    } else {
        Ok(auto_encode_bytes(
            base_convert_xml(html, url, &Default::default())?.as_slice(),
        ))
    }
}

/// Convert HTML to well-formed XML.
pub fn base_convert_xml(
    html: &str,
    _url: &str,
    _encoding: &Option<String>,
) -> Result<Vec<u8>, Box<dyn Error>> {
    let parser = parse_document(RcDom::default(), Default::default());
    let dom = parser.one(html);
    let mut xml_output = Vec::new();
    serialize_xml(&dom.document, &mut xml_output)?;

    Ok(xml_output)
}

/// Serialize a DOM node into XML.
fn serialize_xml<W: Write>(handle: &Handle, writer: &mut W) -> io::Result<()> {
    match handle.data {
        NodeData::Document => {
            for child in handle.children.borrow().iter() {
                serialize_xml(child, writer)?;
            }
        }
        NodeData::Element {
            ref name,
            ref attrs,
            ..
        } => {
            let sname = qual_name_to_string(name);

            if sname == "html" {
                write!(writer, r#"<{} xmlns="http://www.w3.org/1999/xhtml""#, sname)?;
            } else {
                write!(writer, "<{}", sname)?;
            }

            for attr in attrs.borrow().iter() {
                let attr_name = qual_name_to_string(&attr.name);
                let processed_name = if attr_name.contains(":") {
                    attr_name.replace(":", "")
                } else {
                    attr_name
                };
                write!(
                    writer,
                    " {}=\"{}\"",
                    processed_name,
                    escape_xml(&attr.value)
                )?;
            }

            let children = handle.children.borrow();

            if children.is_empty() {
                write!(writer, " />")?;
            } else {
                write!(writer, ">")?;
                let insert_cdata = sname == "script" && !children.is_empty();

                if insert_cdata {
                    write!(writer, "<![CDATA[")?;
                }

                for child in children.iter() {
                    serialize_xml(child, writer)?;
                }

                if insert_cdata {
                    write!(writer, "]]></{}>", sname)?;
                } else {
                    write!(writer, "</{}>", sname)?;
                }
            }
        }
        NodeData::Text { ref contents } => {
            write!(writer, "{}", escape_xml(&contents.borrow()))?;
        }
        NodeData::Comment { ref contents } => {
            write!(writer, "<!--{}-->", escape_xml(contents.as_ref()))?;
        }
        NodeData::Doctype { ref name, .. } => {
            write!(writer, "<!DOCTYPE {}>", name)?;
        }
        _ => (),
    }
    Ok(())
}

/// Helper function to convert qualified names into a string representation.
fn qual_name_to_string(name: &QualName) -> String {
    if name.ns == ns!(html) {
        name.local.to_string()
    } else {
        format!("{}:{}", name.ns, name.local)
    }
}

/// Escape special characters for XML documents (single-pass).
fn escape_xml(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '"' => result.push_str("&quot;"),
            '\'' => result.push_str("&apos;"),
            _ => result.push(ch),
        }
    }
    result
}
