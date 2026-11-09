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
    source_line: Box<str>,
}

/// The longest source line a diagnostic keeps, in characters.
const LINE_CAP: usize = 200;

impl Diagnostic {
    pub(crate) fn new(
        template: &str,
        src: &str,
        at: usize,
        message: String,
        included_from: Option<(&str, &str, usize)>,
    ) -> Self {
        let (line, column) = line_col(src, at);
        let text = src.lines().nth(line as usize - 1).unwrap_or("");
        Self {
            template: template.into(),
            line,
            column,
            message,
            included_from: included_from.map(|(n, s, a)| (n.into(), line_col(s, a).0)),
            source_line: text.chars().take(LINE_CAP).collect::<String>().into(),
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

    /// The source line the problem is on (its first 200 characters).
    #[must_use]
    pub fn source_line(&self) -> &str {
        &self.source_line
    }

    /// Writes this problem in the style of compiler errors: the message, the
    /// place, the source line, and a caret under the column.
    fn report(&self, out: &mut String) {
        use std::fmt::Write as _;
        let num = self.line.to_string();
        let pad = " ".repeat(num.len());
        let _ = writeln!(out, "error: {}", self.message);
        let _ = writeln!(
            out,
            "{pad}--> {}:{}:{}",
            self.template, self.line, self.column
        );
        if let Some((n, l)) = &self.included_from {
            let _ = writeln!(out, "{pad} = included from {n}:{l}");
        }
        let _ = writeln!(out, "{pad} |");
        let _ = writeln!(out, "{num} | {}", self.source_line.replace('\t', " "));
        let col = (self.column as usize).saturating_sub(1).min(LINE_CAP);
        let _ = writeln!(out, "{pad} | {}^", " ".repeat(col));
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

    /// Every problem in the style of compiler errors, with the source line
    /// and a caret under the column, for a terminal or a log.
    ///
    /// ```
    /// struct Page { title: String }
    /// luminess::view!(Page as "page" { title: text });
    ///
    /// let err = luminess::Engine::<Page>::builder()
    ///     .template("home", "<h1>{{ page.titel }}</h1>")
    ///     .build()
    ///     .unwrap_err();
    /// assert_eq!(err.report(), "\
    /// error: `page` (page) has no field `titel`, did you mean `title`?
    ///  --> home:1:13
    ///   |
    /// 1 | <h1>{{ page.titel }}</h1>
    ///   |             ^
    /// ");
    /// ```
    #[must_use]
    pub fn report(&self) -> String {
        let mut out = String::new();
        for (i, d) in self.diagnostics.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            d.report(&mut out);
        }
        out
    }

    /// [`CompileError::report`] as an HTML fragment (a `<pre>` element, every
    /// character escaped), for a development error page. Never show it to
    /// visitors of a live site: it reveals template source.
    #[must_use]
    pub fn to_html(&self) -> String {
        let mut out = Vec::new();
        out.extend_from_slice(b"<pre class=\"luminess-errors\">");
        crate::escape::html(&self.report(), &mut out);
        out.extend_from_slice(b"</pre>");
        String::from_utf8(out).unwrap_or_default()
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
    /// The output passed the limit set with
    /// [`Builder::max_output`](crate::Builder::max_output). What the render
    /// wrote is removed.
    TooLarge,
}

/// A render that could not start. A block that fails while rendering is not
/// an error: it gets its fallback, and [`Rendered`](crate::Rendered) reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderError {
    kind: RenderErrorKind,
    template: Box<str>,
    limit: usize,
}

impl RenderError {
    pub(crate) fn not_found(name: &str) -> Self {
        Self {
            kind: RenderErrorKind::NotFound,
            template: name.into(),
            limit: 0,
        }
    }

    pub(crate) fn too_large(name: &str, limit: usize) -> Self {
        Self {
            kind: RenderErrorKind::TooLarge,
            template: name.into(),
            limit,
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
            RenderErrorKind::TooLarge => write!(
                f,
                "the output of `{}` passed the limit of {} bytes",
                self.template, self.limit
            ),
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
