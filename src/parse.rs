//! Turns the token stream into a syntax tree. Nothing here knows about views;
//! names are checked by the compiler.

use std::ops::Range;

use crate::lex::{self, Tok};

/// How deep statements and expressions may nest. Deeper input is an error, so
/// no template can exhaust the stack.
pub(crate) const MAX_DEPTH: usize = 64;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Expr {
    pub kind: ExprKind,
    pub at: usize,
}

/// One segment of a path: a name, and arguments when it is a method call.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Seg {
    pub name: String,
    pub at: usize,
    pub args: Option<Vec<Expr>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CmpOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ExprKind {
    /// `a.b.c` or `a.b(1, "x")`: each segment, with arguments when called.
    Path(Vec<Seg>),
    Str(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Not(Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Cmp(CmpOp, Box<Expr>, Box<Expr>),
    Filter {
        base: Box<Expr>,
        name: String,
        args: Vec<Expr>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Node {
    Text(Range<usize>),
    Out(Expr),
    If {
        arms: Vec<(Expr, Vec<Node>)>,
        otherwise: Option<Vec<Node>>,
    },
    For {
        var: String,
        var_at: usize,
        list: Expr,
        body: Vec<Node>,
        otherwise: Option<Vec<Node>>,
    },
    /// `{% set name = value %}`.
    Set {
        name: String,
        name_at: usize,
        value: Expr,
    },
    Include {
        name: String,
        at: usize,
        /// `with expr`: the value the partial sees as its root.
        with: Option<Expr>,
    },
    Try {
        body: Vec<Node>,
        fallback: Vec<Node>,
        at: usize,
    },
    /// `{% extends "name" %}`.
    Extends {
        name: String,
        at: usize,
    },
    /// `{% block name %}...{% endblock %}`.
    Block {
        name: String,
        body: Vec<Node>,
        at: usize,
    },
    /// `{{ super() }}`: the parent's version of the enclosing block.
    Super {
        at: usize,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ParseError {
    pub at: usize,
    pub msg: String,
}

impl From<lex::LexError> for ParseError {
    fn from(e: lex::LexError) -> Self {
        Self {
            at: e.at,
            msg: e.msg.to_owned(),
        }
    }
}

fn err<T>(at: usize, msg: impl Into<String>) -> Result<T, ParseError> {
    Err(ParseError {
        at,
        msg: msg.into(),
    })
}

// ---------------------------------------------------------------------------
// Expressions
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum ET {
    Ident(String),
    Str(String),
    Int(i64),
    Float(f64),
    Dot,
    Pipe,
    LParen,
    RParen,
    Comma,
    Cmp(CmpOp),
    Assign,
}

/// Tokenizes the inside of a tag. `base` is the tag's offset in the source.
fn etokens(src: &str, base: usize) -> Result<Vec<(ET, usize)>, ParseError> {
    let b = src.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        let at = base + i;
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        let two = |x: u8| b.get(i + 1) == Some(&x);
        let (tok, len) = match c {
            b'.' => (ET::Dot, 1),
            b'|' => (ET::Pipe, 1),
            b'(' => (ET::LParen, 1),
            b')' => (ET::RParen, 1),
            b',' => (ET::Comma, 1),
            b'=' if two(b'=') => (ET::Cmp(CmpOp::Eq), 2),
            b'=' => (ET::Assign, 1),
            b'!' if two(b'=') => (ET::Cmp(CmpOp::Ne), 2),
            b'<' if two(b'=') => (ET::Cmp(CmpOp::Le), 2),
            b'>' if two(b'=') => (ET::Cmp(CmpOp::Ge), 2),
            b'<' => (ET::Cmp(CmpOp::Lt), 1),
            b'>' => (ET::Cmp(CmpOp::Gt), 1),
            b'"' | b'\'' => {
                let mut s = String::new();
                let mut j = i + 1;
                let mut run = j;
                loop {
                    match b.get(j) {
                        None => return err(at, "unterminated string"),
                        Some(&q) if q == c => break,
                        Some(b'\\') => {
                            s.push_str(&src[run..j]);
                            let e = match b.get(j + 1) {
                                Some(b'\\') => '\\',
                                Some(b'"') => '"',
                                Some(b'\'') => '\'',
                                Some(b'n') => '\n',
                                Some(b't') => '\t',
                                _ => return err(base + j, "unknown escape in string"),
                            };
                            s.push(e);
                            j += 2;
                            run = j;
                        }
                        Some(_) => j += 1,
                    }
                }
                s.push_str(&src[run..j]);
                (ET::Str(s), j + 1 - i)
            }
            b'0'..=b'9' | b'-' => {
                let mut j = i + 1;
                if c == b'-' && !b.get(j).is_some_and(u8::is_ascii_digit) {
                    return err(at, "unexpected `-`");
                }
                while b.get(j).is_some_and(u8::is_ascii_digit) {
                    j += 1;
                }
                let float = b.get(j) == Some(&b'.') && b.get(j + 1).is_some_and(u8::is_ascii_digit);
                if float {
                    j += 1;
                    while b.get(j).is_some_and(u8::is_ascii_digit) {
                        j += 1;
                    }
                    match src[i..j].parse::<f64>() {
                        Ok(f) => (ET::Float(f), j - i),
                        Err(_) => return err(at, "invalid number"),
                    }
                } else {
                    match src[i..j].parse::<i64>() {
                        Ok(n) => (ET::Int(n), j - i),
                        Err(_) => return err(at, "integer out of range"),
                    }
                }
            }
            c if c.is_ascii_alphabetic() || c == b'_' => {
                let mut j = i + 1;
                while b
                    .get(j)
                    .is_some_and(|&x| x.is_ascii_alphanumeric() || x == b'_')
                {
                    j += 1;
                }
                (ET::Ident(src[i..j].to_owned()), j - i)
            }
            _ => {
                let ch = src[i..].chars().next().unwrap_or('?');
                return err(at, format!("unexpected character `{ch}`"));
            }
        };
        out.push((tok, at));
        i += len;
    }
    Ok(out)
}

struct EP {
    toks: Vec<(ET, usize)>,
    pos: usize,
    end: usize,
    depth: usize,
}

impl EP {
    fn peek(&self) -> Option<&ET> {
        self.toks.get(self.pos).map(|t| &t.0)
    }

    fn at(&self) -> usize {
        self.toks.get(self.pos).map_or(self.end, |t| t.1)
    }

    fn bump(&mut self) -> Option<(ET, usize)> {
        let t = self.toks.get(self.pos).cloned();
        self.pos += 1;
        t
    }

    fn is_word(&self, w: &str) -> bool {
        matches!(self.peek(), Some(ET::Ident(s)) if s == w)
    }

    fn enter(&mut self) -> Result<(), ParseError> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return err(self.at(), "expression nested too deeply");
        }
        Ok(())
    }

    fn expr(&mut self) -> Result<Expr, ParseError> {
        self.enter()?;
        let mut left = self.and()?;
        while self.is_word("or") {
            let at = self.at();
            self.pos += 1;
            let right = self.and()?;
            left = Expr {
                kind: ExprKind::Or(Box::new(left), Box::new(right)),
                at,
            };
        }
        self.depth -= 1;
        Ok(left)
    }

    fn and(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.not()?;
        while self.is_word("and") {
            let at = self.at();
            self.pos += 1;
            let right = self.not()?;
            left = Expr {
                kind: ExprKind::And(Box::new(left), Box::new(right)),
                at,
            };
        }
        Ok(left)
    }

    fn not(&mut self) -> Result<Expr, ParseError> {
        if self.is_word("not") {
            let at = self.at();
            self.pos += 1;
            self.enter()?;
            let inner = self.not()?;
            self.depth -= 1;
            return Ok(Expr {
                kind: ExprKind::Not(Box::new(inner)),
                at,
            });
        }
        self.cmp()
    }

    fn cmp(&mut self) -> Result<Expr, ParseError> {
        let left = self.filtered()?;
        if let Some(ET::Cmp(op)) = self.peek() {
            let op = *op;
            let at = self.at();
            self.pos += 1;
            let right = self.filtered()?;
            if matches!(self.peek(), Some(ET::Cmp(_))) {
                return err(self.at(), "comparisons do not chain; use `and`");
            }
            return Ok(Expr {
                kind: ExprKind::Cmp(op, Box::new(left), Box::new(right)),
                at,
            });
        }
        Ok(left)
    }

    fn filtered(&mut self) -> Result<Expr, ParseError> {
        let mut base = self.primary()?;
        while self.peek() == Some(&ET::Pipe) {
            self.pos += 1;
            let at = self.at();
            let Some((ET::Ident(name), _)) = self.bump() else {
                return err(at, "expected a filter name after `|`");
            };
            let args = if self.peek() == Some(&ET::LParen) {
                self.args()?
            } else {
                Vec::new()
            };
            base = Expr {
                kind: ExprKind::Filter {
                    base: Box::new(base),
                    name,
                    args,
                },
                at,
            };
        }
        Ok(base)
    }

    /// `( expr, ... )`, with the `(` next.
    fn args(&mut self) -> Result<Vec<Expr>, ParseError> {
        self.pos += 1;
        let mut args = Vec::new();
        if self.peek() == Some(&ET::RParen) {
            self.pos += 1;
            return Ok(args);
        }
        loop {
            args.push(self.expr()?);
            match self.bump() {
                Some((ET::Comma, _)) => {}
                Some((ET::RParen, _)) => return Ok(args),
                Some((_, a)) => return err(a, "expected `,` or `)`"),
                None => return err(self.end, "expected `)`"),
            }
        }
    }

    /// One path segment's call arguments, if `(` follows.
    fn seg(&mut self, name: String, at: usize) -> Result<Seg, ParseError> {
        let args = if self.peek() == Some(&ET::LParen) {
            Some(self.args()?)
        } else {
            None
        };
        Ok(Seg { name, at, args })
    }

    fn primary(&mut self) -> Result<Expr, ParseError> {
        let at = self.at();
        match self.bump() {
            Some((ET::Str(s), _)) => Ok(Expr {
                kind: ExprKind::Str(s),
                at,
            }),
            Some((ET::Int(n), _)) => Ok(Expr {
                kind: ExprKind::Int(n),
                at,
            }),
            Some((ET::Float(f), _)) => Ok(Expr {
                kind: ExprKind::Float(f),
                at,
            }),
            Some((ET::LParen, _)) => {
                let e = self.expr()?;
                match self.bump() {
                    Some((ET::RParen, _)) => Ok(e),
                    _ => err(at, "unclosed `(`"),
                }
            }
            Some((ET::Ident(w), _)) => match w.as_str() {
                "true" => Ok(Expr {
                    kind: ExprKind::Bool(true),
                    at,
                }),
                "false" => Ok(Expr {
                    kind: ExprKind::Bool(false),
                    at,
                }),
                "and" | "or" | "not" | "in" => err(at, format!("expected a value, found `{w}`")),
                _ => {
                    let mut segs = vec![self.seg(w, at)?];
                    while self.peek() == Some(&ET::Dot) {
                        self.pos += 1;
                        let sat = self.at();
                        match self.bump() {
                            Some((ET::Ident(n), _)) => segs.push(self.seg(n, sat)?),
                            _ => return err(sat, "expected a field name after `.`"),
                        }
                    }
                    Ok(Expr {
                        kind: ExprKind::Path(segs),
                        at,
                    })
                }
            },
            Some((t, a)) => err(a, format!("expected a value, found {}", describe(&t))),
            None => err(at, "expected a value"),
        }
    }

    fn finish(&self) -> Result<(), ParseError> {
        match self.toks.get(self.pos) {
            None => Ok(()),
            Some((t, a)) => err(
                *a,
                format!("unexpected {} after the expression", describe(t)),
            ),
        }
    }
}

fn describe(t: &ET) -> String {
    match t {
        ET::Ident(s) => format!("`{s}`"),
        ET::Str(_) => "a string".into(),
        ET::Int(_) | ET::Float(_) => "a number".into(),
        ET::Dot => "`.`".into(),
        ET::Pipe => "`|`".into(),
        ET::LParen => "`(`".into(),
        ET::RParen => "`)`".into(),
        ET::Comma => "`,`".into(),
        ET::Cmp(_) => "a comparison".into(),
        ET::Assign => "`=`".into(),
    }
}

/// Parses a whole expression from `src`, which starts at `base` in the template.
pub(crate) fn parse_expr(src: &str, base: usize) -> Result<Expr, ParseError> {
    let toks = etokens(src, base)?;
    let mut p = EP {
        toks,
        pos: 0,
        end: base + src.len(),
        depth: 0,
    };
    if p.toks.is_empty() {
        return err(base, "empty expression");
    }
    let e = p.expr()?;
    p.finish()?;
    Ok(e)
}

// ---------------------------------------------------------------------------
// Statements
// ---------------------------------------------------------------------------

/// The tag that ended a block.
#[derive(Debug)]
enum Stop {
    Eof,
    Elif(Expr, usize),
    Else(usize),
    EndIf(usize),
    EndFor(usize),
    Fallback(usize),
    EndTry(usize),
    /// `{% endblock %}`, with the name it repeats, if any.
    EndBlock(usize, Option<String>),
}

impl Stop {
    fn name(&self) -> &'static str {
        match self {
            Self::Eof => "the end of the template",
            Self::Elif(..) => "`{% elif %}`",
            Self::Else(_) => "`{% else %}`",
            Self::EndIf(_) => "`{% endif %}`",
            Self::EndFor(_) => "`{% endfor %}`",
            Self::Fallback(_) => "`{% fallback %}`",
            Self::EndTry(_) => "`{% endtry %}`",
            Self::EndBlock(..) => "`{% endblock %}`",
        }
    }

    fn at(&self) -> usize {
        match self {
            Self::Eof => usize::MAX,
            Self::Elif(_, a)
            | Self::Else(a)
            | Self::EndIf(a)
            | Self::EndFor(a)
            | Self::Fallback(a)
            | Self::EndTry(a)
            | Self::EndBlock(a, _) => *a,
        }
    }
}

struct P<'s> {
    src: &'s str,
    toks: Vec<Tok>,
    pos: usize,
}

impl P<'_> {
    fn block(&mut self, depth: usize) -> Result<(Vec<Node>, Stop), ParseError> {
        let mut nodes = Vec::new();
        while self.pos < self.toks.len() {
            let tok = self.toks[self.pos].clone();
            self.pos += 1;
            match tok {
                Tok::Text(r) => nodes.push(Node::Text(r)),
                Tok::Expr { inner, at } if self.src[inner.clone()].trim() == "super()" => {
                    nodes.push(Node::Super { at });
                }
                Tok::Expr { inner, at } => {
                    let e = parse_expr(&self.src[inner.clone()], inner.start).map_err(|e| {
                        if e.msg == "empty expression" {
                            ParseError { at, ..e }
                        } else {
                            e
                        }
                    })?;
                    nodes.push(Node::Out(e));
                }
                Tok::Stmt { inner, at } => {
                    if let Some(stop) = self.stmt(inner, at, depth, &mut nodes)? {
                        return Ok((nodes, stop));
                    }
                }
            }
        }
        Ok((nodes, Stop::Eof))
    }

    /// Handles one statement tag: pushes a node, or returns the stop tag.
    fn stmt(
        &mut self,
        inner: Range<usize>,
        at: usize,
        depth: usize,
        nodes: &mut Vec<Node>,
    ) -> Result<Option<Stop>, ParseError> {
        let text = &self.src[inner.clone()];
        let trimmed = text.trim_start();
        let lead = text.len() - trimmed.len();
        let word_len = trimmed
            .bytes()
            .position(|c| !(c.is_ascii_alphanumeric() || c == b'_'))
            .unwrap_or(trimmed.len());
        let word = &trimmed[..word_len];
        let rest_off = inner.start + lead + word_len;
        let rest = &self.src[rest_off..inner.end];
        let no_args = |what: &str| -> Result<(), ParseError> {
            if rest.trim().is_empty() {
                Ok(())
            } else {
                err(rest_off, format!("`{what}` takes nothing after it"))
            }
        };
        let child = depth + 1;
        if matches!(word, "if" | "for" | "try" | "block") && child > MAX_DEPTH {
            return err(at, "statements nested too deeply");
        }
        match word {
            "" => err(at, "empty statement `{% %}`"),
            "if" => {
                let mut cond = parse_expr(rest, rest_off)?;
                let mut arms = Vec::new();
                loop {
                    let (body, stop) = self.block(child)?;
                    arms.push((cond, body));
                    match stop {
                        Stop::Elif(e, _) => cond = e,
                        Stop::Else(_) => {
                            let (body, stop) = self.block(child)?;
                            return match stop {
                                Stop::EndIf(_) => {
                                    nodes.push(Node::If {
                                        arms,
                                        otherwise: Some(body),
                                    });
                                    Ok(None)
                                }
                                other => Self::mismatch(&other, "`{% endif %}`", at),
                            };
                        }
                        Stop::EndIf(_) => {
                            nodes.push(Node::If {
                                arms,
                                otherwise: None,
                            });
                            return Ok(None);
                        }
                        other => return Self::mismatch(&other, "`{% endif %}`", at),
                    }
                }
            }
            "for" => {
                let toks = etokens(rest, rest_off)?;
                let (var, var_at) = match toks.first() {
                    Some((ET::Ident(v), a)) if !matches!(v.as_str(), "in" | "loop") => {
                        (v.clone(), *a)
                    }
                    Some((ET::Ident(v), a)) if v == "loop" => {
                        return err(*a, "`loop` is reserved for the loop's own values");
                    }
                    _ => return err(rest_off, "expected `for name in list`"),
                };
                match toks.get(1) {
                    Some((ET::Ident(w), _)) if w == "in" => {}
                    _ => return err(rest_off, "expected `for name in list`"),
                }
                let list_at = toks.get(2).map_or(inner.end, |t| t.1);
                let list = parse_expr(&self.src[list_at..inner.end], list_at)?;
                let (body, stop) = self.block(child)?;
                let otherwise = match stop {
                    Stop::EndFor(_) => None,
                    Stop::Else(_) => {
                        let (e, stop) = self.block(child)?;
                        match stop {
                            Stop::EndFor(_) => Some(e),
                            other => return Self::mismatch(&other, "`{% endfor %}`", at),
                        }
                    }
                    other => return Self::mismatch(&other, "`{% endfor %}`", at),
                };
                nodes.push(Node::For {
                    var,
                    var_at,
                    list,
                    body,
                    otherwise,
                });
                Ok(None)
            }
            "try" => {
                no_args("try")?;
                let (body, stop) = self.block(child)?;
                let fallback = match stop {
                    Stop::Fallback(_) => {
                        let (f, stop) = self.block(child)?;
                        match stop {
                            Stop::EndTry(_) => f,
                            other => return Self::mismatch(&other, "`{% endtry %}`", at),
                        }
                    }
                    Stop::EndTry(_) => Vec::new(),
                    other => return Self::mismatch(&other, "`{% endtry %}`", at),
                };
                nodes.push(Node::Try { body, fallback, at });
                Ok(None)
            }
            "include" => {
                let toks = etokens(rest, rest_off)?;
                match toks.as_slice() {
                    [(ET::Str(name), _)] => {
                        nodes.push(Node::Include {
                            name: name.clone(),
                            at,
                            with: None,
                        });
                        Ok(None)
                    }
                    [(ET::Str(name), _), (ET::Ident(w), _), (_, value_at), ..] if w == "with" => {
                        let value = parse_expr(&self.src[*value_at..inner.end], *value_at)?;
                        nodes.push(Node::Include {
                            name: name.clone(),
                            at,
                            with: Some(value),
                        });
                        Ok(None)
                    }
                    _ => err(
                        rest_off,
                        "expected `include \"name\"` or `include \"name\" with value`",
                    ),
                }
            }
            "extends" => {
                let toks = etokens(rest, rest_off)?;
                match toks.as_slice() {
                    [(ET::Str(name), _)] => {
                        nodes.push(Node::Extends {
                            name: name.clone(),
                            at,
                        });
                        Ok(None)
                    }
                    _ => err(rest_off, "expected `extends \"name\"`"),
                }
            }
            "block" => {
                let toks = etokens(rest, rest_off)?;
                let name = match toks.as_slice() {
                    [(ET::Ident(n), _)] => n.clone(),
                    _ => return err(rest_off, "expected `block name`"),
                };
                let (body, stop) = self.block(child)?;
                match stop {
                    Stop::EndBlock(_, None) => {}
                    Stop::EndBlock(a, Some(n)) if n != name => {
                        return err(a, format!("`endblock {n}` closes `block {name}`"));
                    }
                    Stop::EndBlock(..) => {}
                    other => return Self::mismatch(&other, "`{% endblock %}`", at),
                }
                nodes.push(Node::Block { name, body, at });
                Ok(None)
            }
            "endblock" => {
                let toks = etokens(rest, rest_off)?;
                match toks.as_slice() {
                    [] => Ok(Some(Stop::EndBlock(at, None))),
                    [(ET::Ident(n), _)] => Ok(Some(Stop::EndBlock(at, Some(n.clone())))),
                    _ => err(rest_off, "expected `endblock` or `endblock name`"),
                }
            }
            "set" => {
                let toks = etokens(rest, rest_off)?;
                match toks.as_slice() {
                    [(ET::Ident(n), name_at), (ET::Assign, _), (_, value_at), ..]
                        if !matches!(
                            n.as_str(),
                            "loop" | "true" | "false" | "and" | "or" | "not" | "in"
                        ) =>
                    {
                        let value = parse_expr(&self.src[*value_at..inner.end], *value_at)?;
                        nodes.push(Node::Set {
                            name: n.clone(),
                            name_at: *name_at,
                            value,
                        });
                        Ok(None)
                    }
                    _ => err(rest_off, "expected `set name = value`"),
                }
            }
            "elif" => Ok(Some(Stop::Elif(parse_expr(rest, rest_off)?, at))),
            "else" => no_args("else").map(|()| Some(Stop::Else(at))),
            "endif" => no_args("endif").map(|()| Some(Stop::EndIf(at))),
            "endfor" => no_args("endfor").map(|()| Some(Stop::EndFor(at))),
            "fallback" => no_args("fallback").map(|()| Some(Stop::Fallback(at))),
            "endtry" => no_args("endtry").map(|()| Some(Stop::EndTry(at))),
            "endraw" => err(at, "`{% endraw %}` without `{% raw %}`"),
            "end" => err(
                at,
                "unknown statement `end`; close with `endif`, `endfor`, or `endtry`",
            ),
            "macro" => err(at, format!("`{word}` is not supported yet")),
            other => err(at, format!("unknown statement `{other}`")),
        }
    }

    fn mismatch<T>(got: &Stop, want: &str, open: usize) -> Result<T, ParseError> {
        let at = if matches!(got, Stop::Eof) {
            open
        } else {
            got.at()
        };
        err(at, format!("expected {want}, found {}", got.name()))
    }
}

/// Parses a whole template.
pub(crate) fn parse(src: &str) -> Result<Vec<Node>, ParseError> {
    let toks = lex::lex(src)?;
    let mut p = P { src, toks, pos: 0 };
    let (nodes, stop) = p.block(0)?;
    match stop {
        Stop::Eof => Ok(nodes),
        other => err(
            other.at(),
            format!("{} without an opening statement", other.name()),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(s: &str) -> Expr {
        parse_expr(s, 0).unwrap()
    }

    #[test]
    fn exprs() {
        assert!(matches!(e("a.b.c").kind, ExprKind::Path(ref p) if p.len() == 3));
        assert!(matches!(e("not a and b").kind, ExprKind::And(..)));
        assert!(matches!(e("a or b and c").kind, ExprKind::Or(..)));
        assert!(matches!(e("a == 'x'").kind, ExprKind::Cmp(CmpOp::Eq, ..)));
        assert!(matches!(
            e("a | default(\"x\") | upper").kind,
            ExprKind::Filter { .. }
        ));
        assert!(matches!(e("-3").kind, ExprKind::Int(-3)));
        assert!(matches!(e("2.5").kind, ExprKind::Float(_)));
        assert!(matches!(e("(a)").kind, ExprKind::Path(_)));
        for bad in [
            "",
            "a.",
            "a |",
            "a == b == c",
            "(a",
            "a b",
            "'x",
            "and",
            "a | f(",
            "@",
            "-x",
        ] {
            assert!(parse_expr(bad, 0).is_err(), "{bad}");
        }
    }

    #[test]
    fn statements() {
        let n = parse("{% if a %}x{% elif b %}y{% else %}z{% endif %}").unwrap();
        assert!(matches!(&n[0], Node::If { arms, otherwise: Some(_) } if arms.len() == 2));
        let n = parse("{% for p in page.posts %}{{ p }}{% else %}none{% endfor %}").unwrap();
        assert!(matches!(&n[0], Node::For { var, otherwise: Some(_), .. } if var == "p"));
        let n = parse("{% try %}a{% fallback %}b{% endtry %}{% include \"x\" %}").unwrap();
        assert_eq!(n.len(), 2);
        for bad in [
            "{% if a %}",
            "{% endif %}",
            "{% if a %}{% endfor %}",
            "{% for in x %}{% endfor %}",
            "{% for loop in x %}{% endfor %}",
            "{% else x %}",
            "{% include x %}",
            "{% nope %}",
            "{{ }}",
            "{% %}",
            "{% try %}{% else %}{% endtry %}",
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn depth_is_bounded() {
        let deep = "{% if a %}".repeat(MAX_DEPTH + 1) + &"{% endif %}".repeat(MAX_DEPTH + 1);
        assert!(parse(&deep).is_err());
        let ok = "{% if a %}".repeat(MAX_DEPTH) + &"{% endif %}".repeat(MAX_DEPTH);
        assert!(parse(&ok).is_ok());
        let parens = "(".repeat(200) + "a" + &")".repeat(200);
        assert!(parse_expr(&parens, 0).is_err());
        let nots = "not ".repeat(200) + "a";
        assert!(parse_expr(&nots, 0).is_err());
    }
}
