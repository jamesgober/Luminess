//! Splits a template into literal text, `{{ }}` output tags, and `{% %}`
//! statement tags. Comments disappear here, `{% raw %}` blocks become literal
//! text, and the `-` trim markers are applied.

use std::ops::Range;

/// One piece of a template. Ranges are byte offsets into the source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Tok {
    /// Literal text, copied to the output.
    Text(Range<usize>),
    /// The inside of `{{ ... }}`; `at` is the offset of the opening brace.
    Expr { inner: Range<usize>, at: usize },
    /// The inside of `{% ... %}`.
    Stmt { inner: Range<usize>, at: usize },
}

/// A lexing error at a byte offset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LexError {
    pub at: usize,
    pub msg: &'static str,
}

fn is_ws(c: u8) -> bool {
    c.is_ascii_whitespace()
}

/// The offset of the next `{{`, `{%`, or `{#` at or after `from`.
fn find_open(b: &[u8], mut from: usize) -> Option<usize> {
    while from + 1 < b.len() {
        let off = b[from..b.len() - 1].iter().position(|&c| c == b'{')? + from;
        if matches!(b[off + 1], b'{' | b'%' | b'#') {
            return Some(off);
        }
        from = off + 1;
    }
    None
}

/// The offset of `close` followed by `}` at or after `from`. Inside tags (not
/// comments) quoted strings are skipped, so `"}}"` in a literal is fine.
fn find_close(
    b: &[u8],
    from: usize,
    close: u8,
    strings: bool,
    at: usize,
) -> Result<usize, LexError> {
    let mut i = from;
    while i < b.len() {
        let c = b[i];
        if strings && (c == b'"' || c == b'\'') {
            let q = c;
            i += 1;
            loop {
                match b.get(i) {
                    None => {
                        return Err(LexError {
                            at,
                            msg: "unterminated string in tag",
                        });
                    }
                    Some(b'\\') => i += 2,
                    Some(&x) if x == q => break,
                    Some(_) => i += 1,
                }
            }
            i += 1;
            continue;
        }
        if c == close && b.get(i + 1) == Some(&b'}') {
            return Ok(i);
        }
        i += 1;
    }
    Err(LexError {
        at,
        msg: match close {
            b'}' => "unclosed `{{`",
            b'%' => "unclosed `{%`",
            _ => "unclosed comment `{#`",
        },
    })
}

struct Lexer<'s> {
    b: &'s [u8],
    toks: Vec<Tok>,
    /// The previous tag ended with `-`: strip the next text's leading whitespace.
    trim_next: bool,
}

impl Lexer<'_> {
    fn push_text(&mut self, mut r: Range<usize>) {
        if self.trim_next {
            while r.start < r.end && is_ws(self.b[r.start]) {
                r.start += 1;
            }
            self.trim_next = false;
        }
        if r.start < r.end {
            self.toks.push(Tok::Text(r));
        }
    }

    fn trim_prev(&mut self) {
        if let Some(Tok::Text(r)) = self.toks.last_mut() {
            while r.start < r.end && is_ws(self.b[r.end - 1]) {
                r.end -= 1;
            }
            if r.start == r.end {
                let _ = self.toks.pop();
            }
        }
    }

    /// Finds `{% endraw %}` (with optional trim markers) from `from`. Returns
    /// (offset of `{%`, trims left, offset after `%}`, trims right).
    fn find_endraw(&self, mut from: usize) -> Option<(usize, bool, usize, bool)> {
        let b = self.b;
        while let Some(off) = find_open(b, from) {
            from = off + 1;
            if b[off + 1] != b'%' {
                continue;
            }
            let mut j = off + 2;
            let tl = b.get(j) == Some(&b'-');
            if tl {
                j += 1;
            }
            while j < b.len() && is_ws(b[j]) {
                j += 1;
            }
            if !b[j..].starts_with(b"endraw") {
                continue;
            }
            j += 6;
            while j < b.len() && is_ws(b[j]) {
                j += 1;
            }
            let tr = b.get(j) == Some(&b'-');
            if tr {
                j += 1;
            }
            if b[j..].starts_with(b"%}") {
                return Some((off, tl, j + 2, tr));
            }
        }
        None
    }
}

/// Lexes a whole template.
pub(crate) fn lex(src: &str) -> Result<Vec<Tok>, LexError> {
    let b = src.as_bytes();
    let mut lx = Lexer {
        b,
        toks: Vec::new(),
        trim_next: false,
    };
    let mut i = 0;
    while let Some(off) = find_open(b, i) {
        lx.push_text(i..off);
        let kind = b[off + 1];
        let mut j = off + 2;
        if b.get(j) == Some(&b'-') {
            j += 1;
            lx.trim_prev();
        }
        let close = match kind {
            b'{' => b'}',
            b'%' => b'%',
            _ => b'#',
        };
        let end = find_close(b, j, close, kind != b'#', off)?;
        let trim_right = end > j && b[end - 1] == b'-';
        let inner = j..if trim_right { end - 1 } else { end };
        i = end + 2;
        lx.trim_next = trim_right;
        match kind {
            b'{' => lx.toks.push(Tok::Expr { inner, at: off }),
            b'%' => {
                if src[inner.clone()].trim() == "raw" {
                    let (eoff, tl, after, tr) = lx.find_endraw(i).ok_or(LexError {
                        at: off,
                        msg: "`{% raw %}` without `{% endraw %}`",
                    })?;
                    let mut body = i..eoff;
                    if lx.trim_next {
                        while body.start < body.end && is_ws(b[body.start]) {
                            body.start += 1;
                        }
                        lx.trim_next = false;
                    }
                    if tl {
                        while body.start < body.end && is_ws(b[body.end - 1]) {
                            body.end -= 1;
                        }
                    }
                    if body.start < body.end {
                        lx.toks.push(Tok::Text(body));
                    }
                    i = after;
                    lx.trim_next = tr;
                } else {
                    lx.toks.push(Tok::Stmt { inner, at: off });
                }
            }
            _ => {}
        }
    }
    lx.push_text(i..b.len());
    Ok(lx.toks)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn show(src: &str) -> Vec<String> {
        lex(src)
            .unwrap()
            .into_iter()
            .map(|t| match t {
                Tok::Text(r) => format!("T({})", &src[r]),
                Tok::Expr { inner, .. } => format!("E({})", &src[inner]),
                Tok::Stmt { inner, .. } => format!("S({})", &src[inner]),
            })
            .collect()
    }

    #[test]
    fn basic() {
        assert_eq!(
            show("a{{ x }}b{% if y %}c{# no #}d"),
            ["T(a)", "E( x )", "T(b)", "S( if y )", "T(c)", "T(d)"]
        );
        assert_eq!(show("{ a } {x}"), ["T({ a } {x})"]);
        assert_eq!(show(""), Vec::<String>::new());
    }

    #[test]
    fn strings_may_hold_closers() {
        assert_eq!(
            show(r#"{{ x | default("}}") }}"#),
            [r#"E( x | default("}}") )"#]
        );
        assert_eq!(show(r"{{ 'a\'}}' }}"), [r"E( 'a\'}}' )"]);
    }

    #[test]
    fn trim() {
        assert_eq!(show("a  {{- x -}}  b"), ["T(a)", "E( x )", "T(b)"]);
        assert_eq!(show("  {%- if x -%}\n  "), ["S( if x )"]);
        assert_eq!(show("a \n{#- c -#}\n b"), ["T(a)", "T(b)"]);
    }

    #[test]
    fn raw() {
        assert_eq!(show("{% raw %}{{ x }}{% endraw %}"), ["T({{ x }})"]);
        assert_eq!(
            show("a{% raw -%} {{ x }} {%- endraw %}b"),
            ["T(a)", "T({{ x }})", "T(b)"]
        );
        assert_eq!(
            show("{% raw %}{% endraw x %}{%endraw%}"),
            ["T({% endraw x %})"]
        );
    }

    #[test]
    fn errors() {
        assert_eq!(lex("ab{{ x").unwrap_err().at, 2);
        assert!(lex("{% x").is_err());
        assert!(lex("{# x").is_err());
        assert!(lex("{{ 'x }}").is_err());
        assert!(lex("{% raw %}x").is_err());
    }
}
