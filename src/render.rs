//! Runs a compiled template against a view, writing into the caller's buffer.

use std::{borrow::Cow, fmt::Write as _, sync::Arc};

use crate::{
    compile::{CExpr, CNode, Esc, Filt, LoopAttr, Program, Step},
    escape,
    output::{Failed, Output},
    parse::CmpOp,
    view::{Computed, FieldId, FilterFn, List, Value, View},
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
    failures: Vec<Failed>,
}

impl Rendered {
    /// How many block failures one render keeps for [`Rendered::failures`].
    pub const MAX_FAILURES: usize = 16;

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
        self.failures.first()
    }

    /// The block failures, in the order they happened: at most
    /// [`Rendered::MAX_FAILURES`]; [`Rendered::fallbacks`] counts them all.
    #[must_use]
    pub fn failures(&self) -> &[Failed] {
        &self.failures
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
    /// A `set` value that is text a filter built: its index in `Run::texts`.
    text: Option<usize>,
}

/// A value during evaluation: a view's value, or text a filter produced.
enum Val<'a> {
    V(Value<'a>),
    Owned(String),
}

struct Run<'a, 'o> {
    root: &'a dyn View,
    filters: &'a [Arc<dyn FilterFn>],
    slots: Vec<Slot<'a>>,
    texts: Vec<String>,
    out: &'o mut Vec<u8>,
    scratch: String,
    fallbacks: u32,
    failures: Vec<Failed>,
    /// Where this render's output starts, and the most it may write.
    start: usize,
    limit: usize,
    /// The limit was passed: stop.
    over: bool,
}

/// Renders `program` for `root` into `out`.
/// `None` when the output passed `limit` bytes; what was written is removed.
pub(crate) fn run(
    program: &Program,
    filters: &[Arc<dyn FilterFn>],
    root: &dyn View,
    out: &mut Vec<u8>,
    limit: usize,
) -> Option<Rendered> {
    let start = out.len();
    let mut r = Run {
        root,
        filters,
        slots: vec![
            Slot {
                value: Value::Absent,
                index: 0,
                len: 0,
                text: None,
            };
            program.slots
        ],
        texts: Vec::new(),
        out,
        scratch: String::new(),
        fallbacks: 0,
        failures: Vec::new(),
        start,
        limit,
        over: false,
    };
    r.nodes(&program.nodes);
    if r.over || r.out.len() - start > limit {
        r.out.truncate(start);
        return None;
    }
    Some(Rendered {
        bytes: r.out.len() - start,
        fallbacks: r.fallbacks,
        failures: r.failures,
    })
}

/// A value seen through a borrow, for passing to a method or filter.
fn as_value<'v>(v: &'v Val<'_>) -> Value<'v> {
    match v {
        Val::V(v) => *v,
        Val::Owned(s) => Value::Text(s),
    }
}

/// A value that may borrow a temporary, made independent of it.
fn detach(v: Value<'_>) -> Val<'static> {
    match v {
        Value::Absent | Value::View(_) | Value::List(_) => Val::V(Value::Absent),
        Value::Bool(b) => Val::V(Value::Bool(b)),
        Value::Int(i) => Val::V(Value::Int(i)),
        Value::Float(f) => Val::V(Value::Float(f)),
        Value::Text(s) | Value::Html(s) => Val::Owned(s.to_owned()),
        Value::Display(d) => Val::Owned(d.to_string()),
    }
}

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// Formats a Unix time (seconds, UTC) with `%Y %m %d %H %M %S %b %B %%`.
pub(crate) fn format_date(secs: i64, fmt: &str, out: &mut String) {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Days to a civil date (H. Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    let mut chars = fmt.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        let _ = match chars.next() {
            Some('Y') => write!(out, "{year:04}"),
            Some('m') => write!(out, "{month:02}"),
            Some('d') => write!(out, "{day:02}"),
            Some('H') => write!(out, "{:02}", rem / 3600),
            Some('M') => write!(out, "{:02}", rem / 60 % 60),
            Some('S') => write!(out, "{:02}", rem % 60),
            Some('b') => write!(
                out,
                "{}",
                &MONTHS[usize::try_from(month - 1).unwrap_or(0)][..3]
            ),
            Some('B') => write!(out, "{}", MONTHS[usize::try_from(month - 1).unwrap_or(0)]),
            Some('%') => write!(out, "%"),
            Some(other) => write!(out, "%{other}"),
            None => write!(out, "%"),
        };
    }
}

/// Writes `digits` (an optional `-`, digits, an optional fraction) with a
/// comma between each group of three integer digits.
pub(crate) fn group_digits(digits: &str, out: &mut String) {
    let (sign, rest) = digits.strip_prefix('-').map_or(("", digits), |r| ("-", r));
    let (int, frac) = rest.find('.').map_or((rest, ""), |i| rest.split_at(i));
    out.push_str(sign);
    for (i, c) in int.chars().enumerate() {
        if i > 0 && (int.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out.push_str(frac);
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
            if self.over || self.out.len() - self.start > self.limit {
                self.over = true;
                return;
            }
            match n {
                CNode::Lit(s) => self.out.extend_from_slice(s.as_bytes()),
                CNode::Out(e, esc) => {
                    let v = match self.field_path(e) {
                        Some(v) => Val::V(v),
                        None => self.eval(e),
                    };
                    self.write(v, *esc);
                }
                CNode::Block {
                    expr,
                    esc,
                    fallback,
                } => {
                    if let Val::V(Value::View(v)) = self.eval(expr) {
                        self.block(v, *esc, fallback.as_deref());
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
                                text: None,
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
                        text: None,
                    };
                    self.nodes(body);
                }
                CNode::Set { slot, value } => {
                    let slot_value = match self.eval(value) {
                        Val::V(v) => Slot {
                            value: v,
                            index: 0,
                            len: 1,
                            text: None,
                        },
                        Val::Owned(s) => {
                            self.texts.push(s);
                            Slot {
                                value: Value::Absent,
                                index: 0,
                                len: 1,
                                text: Some(self.texts.len() - 1),
                            }
                        }
                    };
                    self.slots[usize::from(*slot)] = slot_value;
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

    fn block(&mut self, v: &'a dyn View, esc: Esc, fallback: Option<&str>) {
        let mark = self.out.len();
        let result = if esc == Esc::Text {
            v.render(&mut Output::new(self.out))
        } else {
            // Not HTML text: the block's text form, escaped for the place.
            let mut s = std::mem::take(&mut self.scratch);
            s.clear();
            let r = v.text(&mut s);
            if r.is_ok() {
                self.text_out(&s, esc);
            }
            self.scratch = s;
            r
        };
        if let Err(f) = result {
            self.out.truncate(mark);
            match fallback {
                Some(text) => self.text_out(text, if esc == Esc::Text { Esc::Text } else { esc }),
                None if esc == Esc::Text => v.fallback(&mut Output::new(self.out)),
                None => {}
            }
            self.fallbacks = self.fallbacks.saturating_add(1);
            if self.failures.len() < Rendered::MAX_FAILURES {
                self.failures.push(f);
            }
        }
    }

    fn eval(&mut self, e: &'a CExpr) -> Val<'a> {
        match e {
            CExpr::Root(steps) => {
                let root = Val::V(Value::View(self.root));
                if steps.is_empty() {
                    root
                } else {
                    self.walk(root, steps)
                }
            }
            CExpr::Slot(s, steps) => {
                let sl = self.slots[usize::from(*s)];
                let base = match sl.text {
                    Some(i) => Val::Owned(self.texts[i].clone()),
                    None => Val::V(sl.value),
                };
                if steps.is_empty() {
                    base
                } else {
                    self.walk(base, steps)
                }
            }
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
            CExpr::Filter(f, x, args) => {
                let v = self.eval(x);
                match f {
                    Filt::Custom(i) => self.custom(usize::from(*i), v, args),
                    Filt::First | Filt::Last => match v {
                        Val::V(Value::List(l)) if !l.is_empty() => {
                            Val::V(l.get(if *f == Filt::First { 0 } else { l.len() - 1 }))
                        }
                        _ => Val::V(Value::Absent),
                    },
                    Filt::Join => {
                        let Val::V(Value::List(l)) = v else {
                            return Val::V(Value::Absent);
                        };
                        let sep = self.eval(&args[0]);
                        let sep = Self::text(&sep).into_owned();
                        let mut joined = String::new();
                        for i in 0..l.len() {
                            if i > 0 {
                                joined.push_str(&sep);
                            }
                            joined.push_str(&Self::text(&Val::V(l.get(i))));
                        }
                        Val::Owned(joined)
                    }
                    Filt::Truncate | Filt::Replace => {
                        if matches!(v, Val::V(Value::Absent)) {
                            return v;
                        }
                        let s = Self::text(&v).into_owned();
                        let a = self.eval(&args[0]);
                        if *f == Filt::Truncate {
                            let n = match a {
                                Val::V(Value::Int(n)) => usize::try_from(n).unwrap_or(0),
                                _ => 0,
                            };
                            if s.chars().count() <= n {
                                return Val::Owned(s);
                            }
                            let mut cut: String = s.chars().take(n).collect();
                            cut.push_str("...");
                            return Val::Owned(cut);
                        }
                        let from = Self::text(&a).into_owned();
                        let b = self.eval(&args[1]);
                        let to = Self::text(&b);
                        if from.is_empty() {
                            Val::Owned(s)
                        } else {
                            Val::Owned(s.replace(&from, &to))
                        }
                    }
                    Filt::Date => {
                        let Val::V(Value::Int(secs)) = v else {
                            return Val::V(Value::Absent);
                        };
                        let fmt = self.eval(&args[0]);
                        let mut out = String::new();
                        format_date(secs, &Self::text(&fmt), &mut out);
                        Val::Owned(out)
                    }
                    Filt::Number => {
                        let digits = match v {
                            Val::V(Value::Int(i)) => i.to_string(),
                            Val::V(Value::Float(f)) if f.is_finite() => f.to_string(),
                            _ => return v,
                        };
                        let mut out = String::new();
                        group_digits(&digits, &mut out);
                        Val::Owned(out)
                    }
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

    /// The value of a path made only of fields (the common case of an
    /// output), without the general evaluator; `None` for anything else.
    #[inline]
    fn field_path(&self, e: &'a CExpr) -> Option<Value<'a>> {
        let (mut v, steps) = match e {
            CExpr::Root(steps) => (Value::View(self.root), steps),
            CExpr::Slot(s, steps) => {
                let sl = &self.slots[usize::from(*s)];
                if sl.text.is_some() {
                    return None;
                }
                (sl.value, steps)
            }
            _ => return None,
        };
        for step in steps {
            let Step::Field(id) = step else { return None };
            v = match v {
                Value::View(x) => x.field(*id),
                _ => Value::Absent,
            };
        }
        Some(v)
    }

    /// Follows a path from `v`.
    fn walk(&mut self, mut v: Val<'a>, steps: &'a [Step]) -> Val<'a> {
        for step in steps {
            let Val::V(Value::View(x)) = v else {
                return Val::V(Value::Absent);
            };
            v = match step {
                Step::Field(id) => Val::V(x.field(*id)),
                Step::Call(id, args) => self.call(x, *id, args),
            };
        }
        v
    }

    fn call(&mut self, x: &'a dyn View, id: FieldId, args: &'a [CExpr]) -> Val<'a> {
        let vals: Vec<Val<'a>> = args.iter().map(|a| self.eval(a)).collect();
        let values: Vec<Value<'_>> = vals.iter().map(as_value).collect();
        match x.call(id, &values) {
            Computed::Value(v) => Val::V(v),
            Computed::Text(s) => Val::Owned(s),
        }
    }

    fn custom(&mut self, i: usize, v: Val<'a>, args: &'a [CExpr]) -> Val<'a> {
        let vals: Vec<Val<'a>> = args.iter().map(|a| self.eval(a)).collect();
        let values: Vec<Value<'_>> = vals.iter().map(as_value).collect();
        let f = &self.filters[i];
        match v {
            Val::V(value) => match f.apply(value, &values) {
                Computed::Value(v) => Val::V(v),
                Computed::Text(s) => Val::Owned(s),
            },
            Val::Owned(s) => match f.apply(Value::Text(&s), &values) {
                Computed::Value(v) => detach(v),
                Computed::Text(t) => Val::Owned(t),
            },
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
