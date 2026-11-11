//! The buffer a self-rendering block writes into, and the error it returns.

use std::{borrow::Cow, fmt};

use crate::escape;

/// Where a [`View::render`](crate::View::render) writes its markup: the
/// caller's buffer, positioned in HTML text.
///
/// [`Output::text`] escapes; [`Output::markup`] writes trusted markup as is.
/// It also implements [`fmt::Write`], escaping like `text`, so
/// `write!(out, "{price}")` works.
#[derive(Debug)]
pub struct Output<'a> {
    buf: &'a mut Vec<u8>,
}

impl<'a> Output<'a> {
    /// Wraps a buffer. Useful for testing a block's renderer on its own.
    pub fn new(buf: &'a mut Vec<u8>) -> Self {
        Self { buf }
    }

    /// Writes text, escaped for HTML text and quoted attribute values.
    pub fn text(&mut self, s: &str) {
        escape::html(s, self.buf);
    }

    /// Writes markup the caller vouches for, unescaped.
    pub fn markup(&mut self, s: &str) {
        self.buf.extend_from_slice(s.as_bytes());
    }

    /// Writes a URL for a quoted `href` or `src` value: blocked when its scheme
    /// is not allowed, then percent-encoded ([`escape::url`]).
    pub fn url(&mut self, s: &str) {
        if escape::url_allowed(s) {
            escape::url(s, self.buf);
        } else {
            self.buf.extend_from_slice(escape::BLOCKED_URL.as_bytes());
        }
    }

    /// Writes an integer.
    pub fn int(&mut self, i: i64) {
        crate::render::write_int(i, self.buf);
    }

    /// The number of bytes in the whole buffer.
    #[must_use]
    pub fn len(&self) -> usize {
        self.buf.len()
    }

    /// `true` when the whole buffer is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }
}

impl fmt::Write for Output<'_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.text(s);
        Ok(())
    }
}

/// A block that could not render. Its partial output is removed and its
/// fallback written; the reason reaches the caller in [`Rendered`](crate::Rendered).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockError {
    reason: Cow<'static, str>,
}

impl BlockError {
    /// A failure with this reason, which is for logs, never shown to visitors.
    pub fn new(reason: impl Into<Cow<'static, str>>) -> Self {
        Self {
            reason: reason.into(),
        }
    }

    /// The reason.
    #[must_use]
    pub fn reason(&self) -> &str {
        &self.reason
    }
}

impl fmt::Display for BlockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.reason)
    }
}

impl std::error::Error for BlockError {}
