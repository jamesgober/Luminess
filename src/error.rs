//! Compile and render errors.

use std::fmt;

/// One problem in a template, with where it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    template: Box<str>,
    line: u32,
    column: u32,
    message: String,
    included_from: Option<(Box<str>, u32)>,
}

impl Diagnostic {
    pub(crate) fn new(
        template: &str,
        src: &str,
        at: usize,
        message: String,
        included_from: Option<(&str, &str, usize)>,
    ) -> Self {
        let (line, column) = line_col(src, at);
        Self {
            template: template.into(),
            line,
            column,
            message,
            included_from: included_from.map(|(n, s, a)| (n.into(), line_col(s, a).0)),
        }
    }

    /// The name of the template.
    #[must_use]
    pub fn template(&self) -> &str {
        &self.template
    }

    /// The line, from 1.
    #[must_use]
    pub fn line(&self) -> u32 {
        self.line
    }

    /// The column in characters, from 1.
    #[must_use]
    pub fn column(&self) -> u32 {
        self.column
    }

    /// What is wrong.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// The template and line this one was included from, when the problem
    /// shows only there.
    #[must_use]
    pub fn included_from(&self) -> Option<(&str, u32)> {
        self.included_from.as_ref().map(|(n, l)| (&**n, *l))
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}:{}: {}",
            self.template, self.line, self.column, self.message
        )?;
        if let Some((n, l)) = &self.included_from {
            write!(f, " (included from {n}:{l})")?;
        }
        Ok(())
    }
}

/// The line and column (both from 1) of byte offset `at`.
fn line_col(src: &str, at: usize) -> (u32, u32) {
    let mut at = at.min(src.len());
    while !src.is_char_boundary(at) {
        at -= 1;
    }
    let before = &src[..at];
    let line = before.bytes().filter(|&b| b == b'\n').count() + 1;
    let start = before.rfind('\n').map_or(0, |i| i + 1);
    let column = before[start..].chars().count() + 1;
    (
        u32::try_from(line).unwrap_or(u32::MAX),
        u32::try_from(column).unwrap_or(u32::MAX),
    )
}

/// Every problem found while compiling a template set. Nothing is compiled
/// unless every template is free of them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompileError {
    diagnostics: Vec<Diagnostic>,
}

impl CompileError {
    pub(crate) fn new(diagnostics: Vec<Diagnostic>) -> Self {
        Self { diagnostics }
    }

    /// The problems, in template order.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let n = self.diagnostics.len();
        write!(f, "{n} template error{}", if n == 1 { "" } else { "s" })?;
        for d in &self.diagnostics {
            write!(f, "\n  {d}")?;
        }
        Ok(())
    }
}

impl std::error::Error for CompileError {}

/// What went wrong in a render.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RenderErrorKind {
    /// No template has the name asked for.
    NotFound,
}

/// A render that could not start. A block that fails while rendering is not
/// an error: it gets its fallback, and [`Rendered`](crate::Rendered) reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderError {
    kind: RenderErrorKind,
    template: Box<str>,
}

impl RenderError {
    pub(crate) fn not_found(name: &str) -> Self {
        Self {
            kind: RenderErrorKind::NotFound,
            template: name.into(),
        }
    }

    /// What went wrong.
    #[must_use]
    pub fn kind(&self) -> RenderErrorKind {
        self.kind
    }

    /// The template name asked for.
    #[must_use]
    pub fn template(&self) -> &str {
        &self.template
    }
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            RenderErrorKind::NotFound => write!(f, "no template named `{}`", self.template),
        }
    }
}

impl std::error::Error for RenderError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions() {
        assert_eq!(line_col("ab\ncd", 0), (1, 1));
        assert_eq!(line_col("ab\ncd", 4), (2, 2));
        assert_eq!(line_col("é\nx", 1), (1, 1));
        assert_eq!(line_col("x", 99), (1, 2));
    }
}
