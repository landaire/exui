//! Element tree of an XML document with source lines, the input of the XUI
//! and string table compilers.

use quick_xml::Reader;
use quick_xml::events::BytesStart;
use quick_xml::events::Event;

use crate::error::CompileError;
use crate::error::SourceLine;

/// One element: its attributes, child elements and the character data
/// directly inside it, unescaped and concatenated.
#[derive(Clone, Debug)]
pub(crate) struct Node {
    pub name: String,
    pub line: SourceLine,
    pub attributes: Vec<Attribute>,
    pub children: Vec<Node>,
    pub text: String,
}

#[derive(Clone, Debug)]
pub(crate) struct Attribute {
    pub name: String,
    pub value: String,
}

impl Node {
    pub fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|attribute| attribute.name == name)
            .map(|attribute| attribute.value.as_str())
    }

    /// Fails on attributes other than `allowed`.
    pub fn expect_attributes(&self, allowed: &[&str]) -> Result<(), CompileError> {
        let unexpected = self
            .attributes
            .iter()
            .find(|attribute| !allowed.contains(&attribute.name.as_str()));
        match unexpected {
            Some(attribute) => Err(CompileError::UnexpectedAttribute {
                line: self.line,
                element: self.name.clone(),
                attribute: attribute.name.clone(),
            }),
            None => Ok(()),
        }
    }

    /// Fails when the element holds child elements.
    pub fn expect_no_children(&self) -> Result<(), CompileError> {
        match self.children.first() {
            Some(child) => Err(CompileError::UnexpectedElement {
                line: child.line,
                element: child.name.clone(),
                parent: self.name.clone(),
            }),
            None => Ok(()),
        }
    }

    /// Fails when the element holds character data other than whitespace.
    pub fn expect_no_text(&self) -> Result<(), CompileError> {
        if self.text.chars().all(char::is_whitespace) {
            return Ok(());
        }

        Err(CompileError::UnexpectedText {
            line: self.line,
            element: self.name.clone(),
        })
    }

    /// Text of an element that holds only text.
    pub fn leaf_text(&self) -> Result<&str, CompileError> {
        self.expect_no_children()?;

        Ok(&self.text)
    }
}

/// Reads the single root element of `text`. A byte order mark, comments,
/// processing instructions and the declaration are skipped; line ends are
/// kept as written.
pub(crate) fn parse(text: &str) -> Result<Node, CompileError> {
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    let mut reader = Reader::from_str(text);
    let mut lines = LineCounter::new(text);
    let mut open: Vec<Node> = Vec::new();
    let mut root: Option<Node> = None;

    loop {
        let start = reader.buffer_position() as usize;
        let line = lines.line_at(start);
        let event = reader.read_event().map_err(|source| CompileError::Xml {
            line: lines.line_at(reader.error_position() as usize),
            source,
        })?;

        match event {
            Event::Start(start) => open.push(element(&start, line)?),
            Event::Empty(start) => {
                let node = element(&start, line)?;
                close(node, &mut open, &mut root)?;
            }
            Event::End(_) => {
                let Some(node) = open.pop() else {
                    unreachable!("the reader checks that end tags match start tags");
                };
                close(node, &mut open, &mut root)?;
            }
            Event::Text(content) => {
                let content = content
                    .unescape()
                    .map_err(|source| CompileError::Xml { line, source })?;
                append_text(&mut open, &content, line)?;
            }
            Event::CData(content) => {
                let content = content.decode().map_err(|source| CompileError::Xml {
                    line,
                    source: source.into(),
                })?;
                append_text(&mut open, &content, line)?;
            }
            Event::Comment(_) | Event::Decl(_) | Event::PI(_) | Event::DocType(_) => {
                // Not part of the content.
            }
            Event::Eof => break,
        }
    }

    if let Some(node) = open.last() {
        return Err(CompileError::UnclosedElement {
            line: node.line,
            element: node.name.clone(),
        });
    }

    root.ok_or(CompileError::NoRootElement)
}

/// Line numbers of byte offsets, for offsets that only grow.
struct LineCounter<'t> {
    text: &'t str,
    offset: usize,
    line: u32,
}

impl<'t> LineCounter<'t> {
    fn new(text: &'t str) -> Self {
        Self {
            text,
            offset: 0,
            line: 1,
        }
    }

    fn line_at(&mut self, offset: usize) -> SourceLine {
        let offset = offset.min(self.text.len());
        if offset > self.offset {
            let newlines = self.text.as_bytes()[self.offset..offset]
                .iter()
                .filter(|byte| **byte == b'\n')
                .count();
            self.line += newlines as u32;
            self.offset = offset;
        }

        SourceLine(self.line)
    }
}

fn element(start: &BytesStart<'_>, line: SourceLine) -> Result<Node, CompileError> {
    let xml_error = |source| CompileError::Xml { line, source };

    let name = String::from_utf8_lossy(start.name().as_ref()).into_owned();
    let mut attributes = Vec::new();
    for attribute in start.attributes() {
        let attribute = attribute.map_err(|source| xml_error(source.into()))?;
        attributes.push(Attribute {
            name: String::from_utf8_lossy(attribute.key.as_ref()).into_owned(),
            value: attribute.unescape_value().map_err(xml_error)?.into_owned(),
        });
    }

    Ok(Node {
        name,
        line,
        attributes,
        children: Vec::new(),
        text: String::new(),
    })
}

fn close(node: Node, open: &mut [Node], root: &mut Option<Node>) -> Result<(), CompileError> {
    if let Some(parent) = open.last_mut() {
        parent.children.push(node);
        return Ok(());
    }

    if root.is_some() {
        return Err(CompileError::MultipleRootElements { line: node.line });
    }
    *root = Some(node);

    Ok(())
}

fn append_text(open: &mut [Node], content: &str, line: SourceLine) -> Result<(), CompileError> {
    match open.last_mut() {
        Some(node) => {
            node.text.push_str(content);
            Ok(())
        }
        None if content.chars().all(char::is_whitespace) => Ok(()),
        None => Err(CompileError::TextOutsideRoot { line }),
    }
}
