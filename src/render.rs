//! Runs a compiled template against a view, writing into the caller's buffer.

use std::{borrow::Cow, fmt::Write as _};

use crate::{
    compile::{CExpr, CNode, Esc, Filt, LoopAttr, Program},
    escape,
    output::{Failed, Output},
    parse::CmpOp,
    view::{FieldId, List, Value, View},
};

/// Writes an integer in decimal.
pub(crate) fn write_int(i: i64, out: &mut Vec<u8>) {
    let mut buf = [0u8; 20];
    let mut n = i.unsigned_abs();
    let mut p = buf.len();
    loop {
        p -= 1;
        buf[p] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    if i < 0 {
        out.push(b'-');
    }
    out.extend_from_slice(&buf[p..]);
}

fn write_float(f: f64, out: &mut Vec<u8>, scratch: &mut String) {
    scratch.clear();
    let _ = write!(scratch, "{f}");
    out.extend_from_slice(scratch.as_bytes());
}

/// What one render reports besides its output.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Rendered {
    bytes: usize,
    fallbacks: u32,
    first_failure: Option<Failed>,
}

impl Rendered {
    /// The number of bytes this render appended to the buffer.
    #[must_use]
    pub fn bytes(&self) -> usize {
        self.bytes
    }

    /// How many blocks failed and were replaced by their fallback.
    #[must_use]
    pub fn fallbacks(&self) -> u32 {
        self.fallbacks
    }

    /// The first block failure, for the log.
    #[must_use]
    pub fn first_failure(&self) -> Option<&Failed> {
        self.first_failure.as_ref()
    }

    /// `true` when no block failed.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.fallbacks == 0
    }
}

#[derive(Clone, Copy)]
struct Slot<'a> {
    value: Value<'a>,
    index: usize,
    len: usize,
}

/// A value during evaluation: a view's value, or text a filter produced.
enum Val<'a> {
    V(Value<'a>),
    Owned(String),
}

struct Run<'a, 'o> {
    root: &'a dyn View,
    slots: Vec<Slot<'a>>,
    out: &'o mut Vec<u8>,
    scratch: String,
    fallbacks: u32,
    first_failure: Option<Failed>,
}

/// Renders `program` for `root` into `out`.
pub(crate) fn run(program: &Program, root: &dyn View, out: &mut Vec<u8>) -> Rendered {
    let start = out.len();
    let mut r = Run {
        root,
        slots: vec![
            Slot {
                value: Value::Absent,
                index: 0,
                len: 0
            };
            program.slots
        ],
        out,
        scratch: String::new(),
        fallbacks: 0,
        first_failure: None,
    };
    r.nodes(&program.nodes);
    Rendered {
        bytes: r.out.len() - start,
        fallbacks: r.fallbacks,
        first_failure: r.first_failure,
    }
}

fn path<'a>(mut v: Value<'a>, ids: &[FieldId]) -> Value<'a> {
    for &id in ids {
        v = match v {
            Value::View(x) => x.field(id),
            _ => return Value::Absent,
        };
    }
    v
}

fn truthy(v: &Val<'_>) -> bool {
    match v {
        Val::V(v) => match *v {
            Value::Absent => false,
            Value::Bool(b) => b,
            Value::Int(i) => i != 0,
            Value::Float(f) => f != 0.0 && !f.is_nan(),
            Value::Text(s) | Value::Html(s) => !s.is_empty(),
            Value::Display(_) | Value::View(_) => true,
            Value::List(l) => !l.is_empty(),
        },
        Val::Owned(s) => !s.is_empty(),
    }
}

impl<'a> Run<'a, '_> {
    fn nodes(&mut self, nodes: &'a [CNode]) {
        for n in nodes {
            match n {
                CNode::Lit(s) => self.out.extend_from_slice(s.as_bytes()),
                CNode::Out(e, esc) => {
                    let v = self.eval(e);
                    self.write(v, *esc);
                }
                CNode::Block(e) => {
                    if let Val::V(Value::View(v)) = self.eval(e) {
                        self.block(v);
                    }
                }
                CNode::If(arms, otherwise) => {
                    let mut done = false;
                    for (c, body) in arms {
                        if truthy(&self.eval(c)) {
                            self.nodes(body);
                            done = true;
                            break;
                        }
                    }
                    if !done {
                        self.nodes(otherwise);
                    }
                }
                CNode::For {
                    list,
                    slot,
                    body,
                    otherwise,
                } => {
                    let l: Option<&'a dyn List> = match self.eval(list) {
                        Val::V(Value::List(l)) => Some(l),
                        _ => None,
                    };
                    let len = l.map_or(0, List::len);
                    if len == 0 {
                        self.nodes(otherwise);
                        continue;
                    }
                    if let Some(l) = l {
                        let s = usize::from(*slot);
                        for i in 0..len {
                            self.slots[s] = Slot {
                                value: l.get(i),
                                index: i,
                                len,
                            };
                            self.nodes(body);
                        }
                    }
                }
                CNode::Bind { slot, value, body } => {
                    let v = match self.eval(value) {
                        Val::V(v) => v,
                        Val::Owned(_) => Value::Absent,
                    };
                    self.slots[usize::from(*slot)] = Slot {
                        value: v,
                        index: 0,
                        len: 1,
                    };
                    self.nodes(body);
                }
                CNode::Try(body, fallback) => {
                    let mark = self.out.len();
                    let before = self.fallbacks;
                    self.nodes(body);
                    if self.fallbacks != before {
                        self.out.truncate(mark);
                        self.nodes(fallback);
                    }
                }
            }
        }
    }

    fn block(&mut self, v: &'a dyn View) {
        let mark = self.out.len();
        let result = v.render(&mut Output::new(self.out));
        if let Err(f) = result {
            self.out.truncate(mark);
            v.fallback(&mut Output::new(self.out));
            self.fallbacks = self.fallbacks.saturating_add(1);
            if self.first_failure.is_none() {
                self.first_failure = Some(f);
            }
        }
    }

    fn eval(&mut self, e: &'a CExpr) -> Val<'a> {
        match e {
            CExpr::Root(ids) => Val::V(if ids.is_empty() {
                Value::View(self.root)
            } else {
                path(self.root.field(ids[0]), &ids[1..])
            }),
            CExpr::Slot(s, ids) => Val::V(path(self.slots[usize::from(*s)].value, ids)),
            CExpr::Loop(s, attr) => {
                let sl = self.slots[usize::from(*s)];
                let to_i = |n: usize| Value::Int(i64::try_from(n).unwrap_or(i64::MAX));
                Val::V(match attr {
                    LoopAttr::Index => to_i(sl.index + 1),
                    LoopAttr::Index0 => to_i(sl.index),
                    LoopAttr::RevIndex => to_i(sl.len - sl.index),
                    LoopAttr::First => Value::Bool(sl.index == 0),
                    LoopAttr::Last => Value::Bool(sl.index + 1 == sl.len),
                    LoopAttr::Length => to_i(sl.len),
                })
            }
            CExpr::Str(s) => Val::V(Value::Text(s)),
            CExpr::Int(i) => Val::V(Value::Int(*i)),
            CExpr::Float(f) => Val::V(Value::Float(*f)),
            CExpr::Bool(b) => Val::V(Value::Bool(*b)),
            CExpr::Not(x) => {
                let v = self.eval(x);
                Val::V(Value::Bool(!truthy(&v)))
            }
            CExpr::And(a, b) => {
                let v = self.eval(a);
                if !truthy(&v) {
                    return Val::V(Value::Bool(false));
                }
                let w = self.eval(b);
                Val::V(Value::Bool(truthy(&w)))
            }
            CExpr::Or(a, b) => {
                let v = self.eval(a);
                if truthy(&v) {
                    return Val::V(Value::Bool(true));
                }
                let w = self.eval(b);
                Val::V(Value::Bool(truthy(&w)))
            }
            CExpr::Cmp(op, a, b) => {
                let x = self.eval(a);
                let y = self.eval(b);
                Val::V(Value::Bool(Self::compare(*op, &x, &y)))
            }
            CExpr::Default(a, b) => {
                let v = self.eval(a);
                let empty = match &v {
                    Val::V(Value::Absent) => true,
                    Val::V(Value::Text(s) | Value::Html(s)) => s.is_empty(),
                    Val::Owned(s) => s.is_empty(),
                    Val::V(_) => false,
                };
                if empty { self.eval(b) } else { v }
            }
            CExpr::Filter(f, x) => {
                let v = self.eval(x);
                match f {
                    Filt::Length => {
                        let n = match &v {
                            Val::V(Value::List(l)) => l.len(),
                            Val::V(Value::Absent) => 0,
                            _ => Self::text(&v).chars().count(),
                        };
                        Val::V(Value::Int(i64::try_from(n).unwrap_or(i64::MAX)))
                    }
                    Filt::Upper | Filt::Lower | Filt::Trim => {
                        if matches!(v, Val::V(Value::Absent)) {
                            return v;
                        }
                        let s = Self::text(&v);
                        Val::Owned(match f {
                            Filt::Upper => s.to_uppercase(),
                            Filt::Lower => s.to_lowercase(),
                            _ => s.trim().to_owned(),
                        })
                    }
                }
            }
        }
    }

    /// The text form of a scalar value.
    fn text<'v>(v: &'v Val<'a>) -> Cow<'v, str> {
        match v {
            Val::V(v) => match *v {
                Value::Text(s) | Value::Html(s) => Cow::Borrowed(s),
                Value::Int(i) => Cow::Owned(i.to_string()),
                Value::Float(f) => Cow::Owned(f.to_string()),
                Value::Bool(b) => Cow::Borrowed(if b { "true" } else { "false" }),
                Value::Display(d) => Cow::Owned(d.to_string()),
                Value::Absent | Value::View(_) | Value::List(_) => Cow::Borrowed(""),
            },
            Val::Owned(s) => Cow::Borrowed(s),
        }
    }

    fn compare(op: CmpOp, x: &Val<'a>, y: &Val<'a>) -> bool {
        use std::cmp::Ordering;
        let absent = |v: &Val<'_>| matches!(v, Val::V(Value::Absent));
        if absent(x) || absent(y) {
            return op == CmpOp::Ne;
        }
        let num = |v: &Val<'_>| match v {
            Val::V(Value::Int(i)) => Some(*i as f64),
            Val::V(Value::Float(f)) => Some(*f),
            _ => None,
        };
        let ord = match (x, y) {
            (Val::V(Value::Int(a)), Val::V(Value::Int(b))) => Some(a.cmp(b)),
            (Val::V(Value::Bool(a)), Val::V(Value::Bool(b))) => Some(a.cmp(b)),
            _ => match (num(x), num(y)) {
                (Some(a), Some(b)) => a.partial_cmp(&b),
                _ => {
                    let a = Self::text(x).into_owned();
                    let b = Self::text(y);
                    Some(a.as_str().cmp(&b))
                }
            },
        };
        match ord {
            None => op == CmpOp::Ne,
            Some(o) => match op {
                CmpOp::Eq => o == Ordering::Equal,
                CmpOp::Ne => o != Ordering::Equal,
                CmpOp::Lt => o == Ordering::Less,
                CmpOp::Le => o != Ordering::Greater,
                CmpOp::Gt => o == Ordering::Greater,
                CmpOp::Ge => o != Ordering::Less,
            },
        }
    }

    fn write(&mut self, v: Val<'a>, esc: Esc) {
        if let Esc::Js { string: false, .. } = esc {
            // A whole JavaScript literal. Negative numbers get spaces so they
            // cannot join an operator before them (`x-{{ n }}`).
            match v {
                Val::V(Value::Absent | Value::View(_) | Value::List(_)) => {
                    self.out.extend_from_slice(b"null");
                }
                Val::V(Value::Bool(b)) => {
                    self.out
                        .extend_from_slice(if b { b"true" } else { b"false" });
                }
                Val::V(Value::Int(i)) => {
                    if i < 0 {
                        self.out.push(b' ');
                        write_int(i, self.out);
                        self.out.push(b' ');
                    } else {
                        write_int(i, self.out);
                    }
                }
                Val::V(Value::Float(f)) => {
                    if f.is_nan() {
                        self.out.extend_from_slice(b"NaN");
                    } else if f.is_infinite() {
                        self.out.extend_from_slice(if f > 0.0 {
                            b"Infinity"
                        } else {
                            b" -Infinity "
                        });
                    } else if f < 0.0 {
                        self.out.push(b' ');
                        write_float(f, self.out, &mut self.scratch);
                        self.out.push(b' ');
                    } else {
                        write_float(f, self.out, &mut self.scratch);
                    }
                }
                other => {
                    let s = Self::text(&other).into_owned();
                    self.text_out(&s, esc);
                }
            }
            return;
        }
        match v {
            Val::V(v) => match v {
                Value::Absent | Value::View(_) | Value::List(_) => {}
                Value::Bool(b) => self
                    .out
                    .extend_from_slice(if b { b"true" } else { b"false" }),
                Value::Int(i) => write_int(i, self.out),
                Value::Float(f) => write_float(f, self.out, &mut self.scratch),
                Value::Text(s) => self.text_out(s, esc),
                Value::Html(s) => {
                    if matches!(esc, Esc::Text | Esc::Raw) {
                        self.out.extend_from_slice(s.as_bytes());
                    } else {
                        self.text_out(s, esc);
                    }
                }
                Value::Display(d) => {
                    let mut s = std::mem::take(&mut self.scratch);
                    s.clear();
                    let _ = write!(s, "{d}");
                    self.text_out(&s, esc);
                    self.scratch = s;
                }
            },
            Val::Owned(s) => self.text_out(&s, esc),
        }
    }

    fn text_out(&mut self, s: &str, esc: Esc) {
        match esc {
            Esc::Text | Esc::Attr => escape::html(s, self.out),
            Esc::UrlStart => {
                if escape::url_allowed(s) {
                    escape::url(s, self.out);
                } else {
                    self.out.extend_from_slice(escape::BLOCKED_URL.as_bytes());
                }
            }
            Esc::UrlPath => escape::url(s, self.out),
            Esc::UrlQuery => escape::url_component(s, self.out),
            Esc::Raw => self.out.extend_from_slice(s.as_bytes()),
            // JavaScript and CSS escaping leaves no byte HTML treats
            // specially, except the quotes around a JavaScript literal, which
            // an attribute value needs as `&quot;`.
            Esc::Js { string: true, .. } => escape::js_string(s, self.out),
            Esc::Js {
                string: false,
                attr,
            } => {
                let q: &[u8] = if attr { b"&quot;" } else { b"\"" };
                self.out.extend_from_slice(q);
                escape::js_string(s, self.out);
                self.out.extend_from_slice(q);
            }
            Esc::Css { string: true, .. } => escape::css(s, self.out),
            Esc::Css { string: false, .. } => {
                if escape::css_value_allowed(s) {
                    self.out.extend_from_slice(s.as_bytes());
                } else {
                    self.out.extend_from_slice(escape::BLOCKED_CSS.as_bytes());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ints() {
        for i in [0, 1, -1, 42, i64::MAX, i64::MIN, 1_000_000] {
            let mut o = Vec::new();
            write_int(i, &mut o);
            assert_eq!(String::from_utf8(o).unwrap(), i.to_string());
        }
    }
}
