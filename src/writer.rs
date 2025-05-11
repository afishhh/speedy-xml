//! A simple XML writer.

use std::{
    fmt::{Debug, Display},
    io::Write,
};

use crate::{
    escape::{comment_escape, content_escape},
    lut::{is_invalid_attribute_name, is_invalid_name},
    reader::{
        self, AttributeEvent, AttributeQuote, CDataEvent, CommentEvent, DoctypeEvent, TextEvent,
    },
};

#[non_exhaustive]
#[derive(Default, Clone)]
/// XML writer options.
pub struct Options {
    /// Whether to ignore all calls to [`Writer::write_comment`] and [`Writer::write_raw_comment`]
    pub omit_comments: bool,
}

/// An XML writer.
pub struct Writer<W: Write> {
    writer: W,
    options: Options,
    depth_and_flags: u32,
}

/// An error that can occur while writing XML.
///
/// This is either caused by passing an incorrectly escaped string to
/// a `write_raw_*` method, writing an attribute with an invalid name or outside of a start tag, or by an underlying I/O error.
pub enum Error {
    /// An invalid prefix was passed to [`Writer::write_start`].
    InvalidElementPrefix,
    /// An invalid name was passed to [`Writer::write_start`].
    InvalidElementName,
    /// An invalid name was passed to [`Writer::write_attribute`] or [`Writer::write_raw_attribute`].
    InvalidAttributeName,
    /// An invalid value was passed to [`Writer::write_raw_attribute`].
    InvalidAttributeValue,
    /// Either [`Writer::write_attribute`] or [`Writer::write_raw_attribute`] was called outside a start tag context.
    AttributeOutsideTag,
    /// Improperly escaped content was passed to [`Writer::write_raw_comment`] or [`Writer::write_raw_text`].
    ImproperlyEscaped,
    /// A string containing `]]>` was passed to [`Writer::write_cdata`].
    InvalidCData,
    /// A string containing a null byte was passed to [`Writer::write_raw_comment`] or [`Writer::write_raw_text`].
    InvalidValue,
    /// An I/O error occured.
    Io(std::io::Error),
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl Debug for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        <Self as Display>::fmt(self, f)
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Error::InvalidElementPrefix => "invalid element prefix",
            Error::InvalidElementName => "invalid element name",
            Error::InvalidAttributeName => "invalid attribute name",
            Error::InvalidAttributeValue => "invalid attribute value",
            Error::AttributeOutsideTag => "attributes are only allowed inside tags",
            Error::ImproperlyEscaped => "improperly escaped content",
            Error::InvalidCData => "cdata content cannot contain `]]>`",
            Error::InvalidValue => "value contains null byte",
            Error::Io(error) => return <std::io::Error as Display>::fmt(error, f),
        })
    }
}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl<W: Write> Writer<W> {
    /// Creates a new [`Writer`] that will write into `writer`.
    #[inline]
    pub fn new(writer: W) -> Self {
        Self::with_options(writer, Options::default())
    }

    /// Creates a new [`Writer`] that will write into `writer` with the specified options.
    #[inline]
    pub fn with_options(writer: W, options: Options) -> Self {
        Self {
            writer,
            options,
            depth_and_flags: 0,
        }
    }

    fn in_empty_tag(&self) -> bool {
        self.depth_and_flags & 0b10 > 0
    }

    fn ensure_tag_closed(&mut self) -> Result<(), std::io::Error> {
        if self.depth_and_flags & 1 > 0 {
            if self.in_empty_tag() {
                self.writer.write_all(b"/>")?;
                self.depth_and_flags += 0b001;
            } else {
                self.writer.write_all(b">")?;
                self.depth_and_flags += 0b011;
            }
        }

        Ok(())
    }

    /// Writes a start tag with the specified `prefix` and `name` into the writer.
    ///
    /// # Errors
    ///
    /// Returns an error if the prefix or name is invalid or an underlying I/O error occurs.
    pub fn write_start(&mut self, prefix: Option<&str>, name: &str) -> Result<(), Error> {
        if prefix.is_some_and(|pfx| pfx.bytes().any(is_invalid_name)) {
            return Err(Error::InvalidElementPrefix);
        }

        if name.bytes().any(is_invalid_name) {
            return Err(Error::InvalidElementName);
        }

        self.ensure_tag_closed()?;

        self.depth_and_flags += 0b1;
        // TODO: write_all_vectored
        self.writer.write_all(b"<")?;
        if let Some(prefix) = prefix {
            self.writer.write_all(prefix.as_bytes())?;
            self.writer.write_all(b":")?;
        }
        self.writer.write_all(name.as_bytes())?;

        Ok(())
    }

    /// Writes an empty tag with the specified `prefix` and `name` into the writer.
    ///
    /// # Errors
    ///
    /// Returns an error if the prefix or name is invalid or an underlying I/O error occurs.
    pub fn write_empty(&mut self, prefix: Option<&str>, name: &str) -> Result<(), Error> {
        if name.bytes().any(is_invalid_name) {
            return Err(Error::InvalidElementName);
        }

        self.ensure_tag_closed()?;

        self.depth_and_flags += 0b11;
        // TODO: write_all_vectored
        self.writer.write_all(b"<")?;
        if let Some(prefix) = prefix {
            self.writer.write_all(prefix.as_bytes())?;
            self.writer.write_all(b":")?;
        }
        self.writer.write_all(name.as_bytes())?;

        Ok(())
    }

    /// Writes an attribute with the specified `prefix` and `name` into the writer.
    ///
    /// The attribute will use the double quote as the quote character.
    /// Does not escape the `value` but will return an error if is improperly escaped.
    ///
    /// Must only be called in the context of a start tag, i.e. after a successful [`Self::write_start`], [`Self::write_empty`], [`Self::write_raw_attribute`], or [`Self::write_attribute`].
    ///
    /// # Errors
    ///
    /// Returns an error if the name or value is invalid or an underlying I/O error occurs.
    pub fn write_raw_attribute(
        &mut self,
        name: &str,
        quote: AttributeQuote,
        value: &str,
    ) -> Result<(), Error> {
        if self.depth_and_flags & 1 == 0 {
            return Err(Error::AttributeOutsideTag);
        }

        if name.bytes().any(is_invalid_attribute_name) {
            return Err(Error::InvalidAttributeName);
        }

        let quote = quote as u8;
        if name.bytes().any(|b| [b'\0', quote].contains(&b)) {
            return Err(Error::InvalidAttributeValue);
        }

        self.writer.write_all(b" ")?;
        self.writer.write_all(name.as_bytes())?;
        self.writer.write_all(b"=")?;
        self.writer.write_all(&[quote])?;
        self.writer.write_all(value.as_bytes())?;
        self.writer.write_all(&[quote])?;

        Ok(())
    }

    /// Writes an attribute with the specified `prefix` and `name` into the writer.
    ///
    /// The attribute will use the double quote as the quote character.
    ///
    /// Must only be called in the context of a start tag, i.e. after a successful [`Self::write_start`], [`Self::write_empty`], [`Self::write_raw_attribute`], or [`Self::write_attribute`].
    ///
    /// # Errors
    ///
    /// Returns an error if the name is invalid or an underlying I/O error occurs.
    pub fn write_attribute(&mut self, name: &str, value: &str) -> Result<(), Error> {
        let escaped = content_escape(value);
        self.write_raw_attribute(name, AttributeQuote::Double, &escaped)
    }

    /// Writes an end tag with the specified `prefix` and `name` into the writer.
    ///
    /// # Errors
    ///
    /// Returns an error if the prefix or name is invalid or an underlying I/O error occurs.
    pub fn write_end(&mut self, prefix: Option<&str>, name: &str) -> Result<(), Error> {
        if prefix.is_some_and(|pfx| pfx.bytes().any(is_invalid_name)) {
            return Err(Error::InvalidElementPrefix);
        }

        if name.bytes().any(is_invalid_name) {
            return Err(Error::InvalidElementName);
        }

        self.ensure_tag_closed()?;

        // TODO: write_all_vectored
        self.writer.write_all(b"</")?;
        if let Some(prefix) = prefix {
            self.writer.write_all(prefix.as_bytes())?;
            self.writer.write_all(b":")?;
        }
        self.writer.write_all(name.as_bytes())?;
        self.writer.write_all(b">")?;

        self.depth_and_flags -= 0b100;

        Ok(())
    }

    fn write_raw_text_unchecked(&mut self, text: &str) -> std::io::Result<()> {
        self.ensure_tag_closed()?;

        self.writer.write_all(text.as_bytes())
    }

    /// Writes text content into the writer.
    ///
    /// # Errors
    ///
    /// Returns an error if the content is improperly escaped or contains a null byte or an underlying I/O error occurs.
    pub fn write_raw_text(&mut self, text: &str) -> Result<(), Error> {
        if let Some(idx) = memchr::memchr2(b'\0', b'<', text.as_bytes()) {
            return Err(if text.as_bytes()[idx] == b'<' {
                Error::ImproperlyEscaped
            } else {
                Error::InvalidValue
            });
        }

        self.write_raw_text_unchecked(text).map_err(Into::into)
    }

    /// Writes text content into the writer.
    ///
    /// # Notes
    ///
    /// Currently this function does not check for null bytes in the string. This may change in a future release.
    ///
    /// # Errors
    ///
    /// Returns an error if an underlying I/O error occurs.
    pub fn write_text(&mut self, content: &str) -> Result<(), Error> {
        let escaped = content_escape(content);
        self.write_raw_text_unchecked(&escaped).map_err(Into::into)
    }

    fn write_cdata_unchecked(&mut self, text: &str) -> std::io::Result<()> {
        self.ensure_tag_closed()?;

        self.writer.write_all(b"<![CDATA[")?;
        self.writer.write_all(text.as_bytes())?;
        self.writer.write_all(b"]]>")
    }

    /// Writes cdata into the writer.
    ///
    /// # Notes
    ///
    /// Currently this function does not check for null bytes in the string. This may change in a future release.
    ///
    /// # Errors
    ///
    /// Returns an error if the string contains `]]>` or an underlying I/O error occurs.
    pub fn write_cdata(&mut self, text: &str) -> Result<(), Error> {
        if memchr::memmem::find(text.as_bytes(), b"]]>").is_some() {
            return Err(Error::InvalidCData);
        }

        self.write_cdata_unchecked(text).map_err(Into::into)
    }

    fn write_raw_comment_unchecked(&mut self, text: &str) -> std::io::Result<()> {
        self.ensure_tag_closed()?;

        self.writer.write_all(b"<!--")?;
        self.writer.write_all(text.as_bytes())?;
        self.writer.write_all(b"-->")?;

        Ok(())
    }

    /// Writes a comment into the writer.
    ///
    /// # Notes
    ///
    /// Currently this function does not check for null bytes in the string. This may change in a future release.
    ///
    /// # Errors
    ///
    /// Returns an error if the string contains `-->` or an underlying I/O error occurs.
    pub fn write_raw_comment(&mut self, text: &str) -> Result<(), Error> {
        if memchr::memmem::find(text.as_bytes(), b"-->").is_some() {
            return Err(Error::ImproperlyEscaped);
        }

        if !self.options.omit_comments {
            self.write_raw_comment_unchecked(text)?
        }

        Ok(())
    }

    /// Writes a comment into the writer.
    ///
    /// # Notes
    ///
    /// Currently this function does not check for null bytes in the string. This may change in a future release.
    ///
    /// # Errors
    ///
    /// Returns an error if an underlying I/O error occurs.
    pub fn write_comment(&mut self, content: &str) -> Result<(), Error> {
        if !self.options.omit_comments {
            let escaped = comment_escape(content);
            self.write_raw_comment_unchecked(&escaped)?
        }

        Ok(())
    }

    /// Writes a comment into the writer.
    ///
    /// # Notes
    ///
    /// Currently this function does not check for null bytes in the string. This may change in a future release.
    ///
    /// # Errors
    ///
    /// Returns an error if an underlying I/O error occurs.
    pub fn write_attribute_event(&mut self, attr: &AttributeEvent) -> Result<(), Error> {
        if self.depth_and_flags & 1 == 0 {
            return Err(Error::AttributeOutsideTag);
        }

        self.writer.write_all(b" ")?;
        self.writer.write_all(attr.name().as_bytes())?;
        self.writer.write_all(b"=")?;
        self.writer.write_all(&[attr.quote() as u8])?;
        self.writer.write_all(attr.raw_value().as_bytes())?;
        self.writer.write_all(&[attr.quote() as u8])?;

        Ok(())
    }

    /// Writes an event into the writer.
    ///
    /// # Errors
    ///
    /// Returns an error if an underlying I/O error occurs.
    pub fn write_event(&mut self, event: &reader::Event) -> Result<(), Error> {
        match event {
            reader::Event::Start(start) | reader::Event::Empty(start) => {
                if start.is_empty() {
                    self.write_empty(start.prefix(), start.name())?;
                } else {
                    self.write_start(start.prefix(), start.name())?;
                }

                for attr in start.attributes() {
                    self.write_attribute_event(&attr)?;
                }

                Ok(())
            }
            reader::Event::End(end) => self.write_end(end.prefix(), end.name()),
            &reader::Event::Comment(CommentEvent { text })
            | &reader::Event::CData(CDataEvent { text })
            | &reader::Event::Doctype(DoctypeEvent { text })
            | &reader::Event::Text(TextEvent { text }) => {
                self.ensure_tag_closed()?;

                self.writer.write_all(text.as_bytes())?;

                Ok(())
            }
        }
    }

    /// Returns a reference to the underlying writer.
    pub fn inner_ref(&self) -> &W {
        &self.writer
    }

    /// Returns a mutable reference to the underlying writer.
    pub fn inner_mut(&mut self) -> &mut W {
        &mut self.writer
    }

    /// If the writer is currently in a start tag context, ensures that the tag is closed, and then returns the underlying writer.
    ///
    /// # Errors
    ///
    /// Returns an error if an underlying I/O error occurred.
    pub fn finish(mut self) -> std::io::Result<W> {
        self.ensure_tag_closed()?;

        Ok(self.writer)
    }

    /// If the writer is currently in a start tag context, ensures that the tag is closed, and then flushes the underlying writer.
    ///
    /// # Errors
    ///
    /// Returns an error if an underlying I/O error occurred.
    pub fn flush(&mut self) -> std::io::Result<()> {
        self.ensure_tag_closed()?;

        self.writer.flush()
    }
}

#[test]
fn reader_writer_roundtrip() {
    const CASES: &[&str] = &[
        "hello world",
        "<some xml='text'/>",
        r#"more stuff<then a_tag="here">with content and <![CDATA[value]]></end>"#,
        "text <!-- something with comments --> text text",
    ];

    for &input in CASES {
        let mut writer = Writer::new(std::io::Cursor::new(Vec::new()));
        let mut reader = reader::Reader::with_options(
            input,
            reader::Options::default().allow_top_level_text(true),
        );

        while let Some(event) = reader.next().transpose().unwrap() {
            dbg!(event);
            writer.write_event(&event).unwrap();
        }

        let result = writer.finish().unwrap().into_inner();

        assert_eq!(std::str::from_utf8(&result).unwrap(), input)
    }
}
