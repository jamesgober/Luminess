//! Compiles a parsed template against a schema: every name is resolved to a
//! field id, every expression is typed, and every output gets the escaper for
//! the HTML context it sits in. Includes are inlined.

use std::collections::HashMap;

use crate::{
    html::{Ctx, Slot, UrlPart},
    parse::{CmpOp, Expr, ExprKind, Node, ParseError},
    view::{FieldId, Kind, Schema},
};

/// How deep includes may nest.
pub(crate) const MAX_INCLUDE_DEPTH: usize = 8;

/// How long an `extends` chain may be.
pub(crate) const MAX_EXTENDS_DEPTH: usize = 8;

/// The most compiled nodes one template may have after includes are inlined.
pub(crate) const MAX_NODES: usize = 1 << 20;

// ---------------------------------------------------------------------------
// Compiled form
// ---------------------------------------------------------------------------

/// How an output is escaped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Esc {
    /// HTML text: text escaped, trusted markup as is.
    Text,
    /// A quoted attribute value or RCDATA: everything escaped.
    Attr,
    /// The start of a URL attribute: scheme checked, then normalized.
    UrlStart,
    /// Inside a URL, before the query: normalized.
    UrlPath,
    /// In the query or fragment: component-encoded.
    UrlQuery,
    /// `| raw`: as is.
    Raw,
    /// JavaScript: a whole literal in code (`string: false`) or string
    /// content; `attr` adds HTML escaping for an `on*` attribute.
    Js { string: bool, attr: bool },
    /// CSS: a checked value in code or escaped string content; `attr` adds
    /// HTML escaping for a `style` attribute.
    Css { string: bool, attr: bool },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LoopAttr {
    Index,
    Index0,
    RevIndex,
    First,
    Last,
    Length,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Filt {
    Upper,
    Lower,
    Trim,
    Length,
}

#[derive(Debug)]
pub(crate) enum CExpr {
    /// The root view, then a field path (empty: the root itself).
    Root(Box<[FieldId]>),
    /// A loop variable, then a field path.
    Slot(u16, Box<[FieldId]>),
    Loop(u16, LoopAttr),
    Str(Box<str>),
    Int(i64),
    Float(f64),
    Bool(bool),
    Not(Box<CExpr>),
    And(Box<CExpr>, Box<CExpr>),
    Or(Box<CExpr>, Box<CExpr>),
    Cmp(CmpOp, Box<CExpr>, Box<CExpr>),
    Filter(Filt, Box<CExpr>),
    Default(Box<CExpr>, Box<CExpr>),
}

#[derive(Debug)]
pub(crate) enum CNode {
    Lit(Box<str>),
    Out(CExpr, Esc),
    Block(CExpr),
    If(Vec<(CExpr, Vec<CNode>)>, Vec<CNode>),
    For {
        list: CExpr,
        slot: u16,
        body: Vec<CNode>,
        otherwise: Vec<CNode>,
    },
    Try(Vec<CNode>, Vec<CNode>),
    /// `include ... with value`: the value goes in `slot` for the body.
    Bind {
        slot: u16,
        value: CExpr,
        body: Vec<CNode>,
    },
}

/// One compiled template.
#[derive(Debug)]
pub(crate) struct Program {
    pub nodes: Vec<CNode>,
    /// The deepest loop nesting: the number of loop slots a render needs.
    pub slots: usize,
}

// ---------------------------------------------------------------------------
// Inputs and diagnostics
// ---------------------------------------------------------------------------

pub(crate) struct Source {
    pub name: Box<str>,
    pub src: Box<str>,
    pub schema: Schema,
    pub ast: Result<Vec<Node>, ParseError>,
}

/// An error before line and column are worked out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Diag {
    pub template: usize,
    pub at: usize,
    pub msg: String,
    /// The template and offset this one was included from.
    pub from: Option<(usize, usize)>,
}

/// The static type of an expression.
#[derive(Clone, Debug)]
enum Ty {
    Bool,
    Int,
    Float,
    Text,
    Html,
    Object(Schema),
    List(Kind),
    /// After an error: accepted everywhere so one mistake reports once.
    Unknown,
}

impl Ty {
    fn of(kind: &Kind) -> Self {
        match kind {
            Kind::Bool => Self::Bool,
            Kind::Int => Self::Int,
            Kind::Float => Self::Float,
            Kind::Text => Self::Text,
            Kind::Html => Self::Html,
            Kind::Object(s) => Self::Object(s.get()),
            Kind::List(k) => Self::List((**k).clone()),
        }
    }

    fn describe(&self) -> &'static str {
        match self {
            Self::Bool => "a bool",
            Self::Int => "an int",
            Self::Float => "a float",
            Self::Text => "text",
            Self::Html => "html",
            Self::Object(_) => "an object",
            Self::List(_) => "a list",
            Self::Unknown => "unknown",
        }
    }

    fn is_number(&self) -> bool {
        matches!(self, Self::Int | Self::Float)
    }

    fn is_text(&self) -> bool {
        matches!(self, Self::Text | Self::Html)
    }
}

struct Var {
    name: String,
    slot: u16,
    item: Ty,
}

struct Scope {
    root_name: String,
    root: Schema,
    /// The slot holding the root value, when it is not the rendered view
    /// (`include ... with value`).
    root_slot: Option<u16>,
    /// The first slot this scope's loops may use.
    base: usize,
    vars: Vec<Var>,
}

impl Scope {
    fn next_slot(&self) -> usize {
        self.base + self.vars.len()
    }
}

/// The blocks of one `extends` chain: for each name, its definitions from the
/// most derived template to the base, with the template each one is in.
struct Frame<'s> {
    defs: HashMap<&'s str, Vec<(usize, &'s [Node])>>,
    /// Blocks being compiled: (name, which definition).
    active: Vec<(&'s str, usize)>,
}

struct Compiler<'s> {
    sources: &'s [Source],
    index: &'s HashMap<&'s str, usize>,
    diags: Vec<Diag>,
    /// Above zero while probing a loop body: diagnostics are dropped.
    quiet: usize,
    /// Includes being compiled: (template, offset of the include tag).
    stack: Vec<(usize, usize)>,
    nodes: usize,
    slots: usize,
    frames: Vec<Frame<'s>>,
}

/// Every block definition in `nodes`, nested ones included.
fn collect_blocks<'s>(nodes: &'s [Node], out: &mut Vec<(&'s str, usize, &'s [Node])>) {
    for n in nodes {
        match n {
            Node::Block { name, body, at } => {
                out.push((name, *at, body));
                collect_blocks(body, out);
            }
            Node::If { arms, otherwise } => {
                for (_, b) in arms {
                    collect_blocks(b, out);
                }
                if let Some(b) = otherwise {
                    collect_blocks(b, out);
                }
            }
            Node::For {
                body, otherwise, ..
            } => {
                collect_blocks(body, out);
                if let Some(b) = otherwise {
                    collect_blocks(b, out);
                }
            }
            Node::Try { body, fallback, .. } => {
                collect_blocks(body, out);
                collect_blocks(fallback, out);
            }
            _ => {}
        }
    }
}

/// Where a node starts, for error messages.
fn node_at(n: &Node) -> Option<usize> {
    match n {
        Node::Text(r) => Some(r.start),
        Node::Out(e) => Some(e.at),
        Node::If { arms, .. } => arms.first().map(|a| a.0.at),
        Node::For { list, .. } => Some(list.at),
        Node::Include { at, .. }
        | Node::Try { at, .. }
        | Node::Extends { at, .. }
        | Node::Block { at, .. }
        | Node::Super { at } => Some(*at),
    }
}

const FILTERS: &str = "default, upper, lower, trim, length, raw";

impl<'s> Compiler<'s> {
    fn error(&mut self, t: usize, at: usize, msg: impl Into<String>) {
        if self.quiet > 0 {
            return;
        }
        let from = self.stack.last().copied();
        self.diags.push(Diag {
            template: t,
            at,
            msg: msg.into(),
            from,
        });
    }

    fn template(&mut self, t: usize) -> Option<Program> {
        let src = &self.sources[t];
        if src.ast.is_err() {
            return None;
        }
        let mut scope = Scope {
            root_name: src.schema.name().to_owned(),
            root: src.schema.clone(),
            root_slot: None,
            base: 0,
            vars: Vec::new(),
        };
        self.nodes = 0;
        self.slots = 0;
        let before = self.diags.len();
        let (root, frame) = self.layout(t)?;
        let Ok(ast) = &self.sources[root].ast else {
            return None;
        };
        self.frames.push(frame);
        let (nodes, end) = self.nodes(root, ast, Ctx::Text, &mut scope);
        let _ = self.frames.pop();
        if end != Ctx::Text && self.diags.len() == before {
            let at = src.src.len();
            self.error(
                t,
                at,
                format!("the template ends inside {}; close it", end.describe()),
            );
        }
        if self.nodes > MAX_NODES {
            self.error(
                t,
                0,
                "the template is too large once its includes are inlined",
            );
        }
        (self.diags.len() == before).then_some(Program {
            nodes,
            slots: self.slots,
        })
    }

    /// Follows `extends` from `t` to the base template. Returns the base and
    /// the blocks of the whole chain, or `None` after reporting an error.
    fn layout(&mut self, t: usize) -> Option<(usize, Frame<'s>)> {
        let sources = self.sources;
        let mut chain = vec![t];
        let mut cur = t;
        loop {
            let Ok(ast) = &sources[cur].ast else {
                return None;
            };
            let Some((name, at)) = self.extends_of(cur, ast)? else {
                break;
            };
            let Some(&parent) = self.index.get(name) else {
                let mut names: Vec<&str> = self.index.keys().copied().collect();
                names.sort_unstable();
                let hint = suggest(name, names.iter().copied());
                self.error(cur, at, format!("unknown template `{name}`{hint}"));
                return None;
            };
            if chain.contains(&parent) {
                self.error(cur, at, format!("`{name}` extends itself"));
                return None;
            }
            if chain.len() > MAX_EXTENDS_DEPTH {
                self.error(cur, at, "`extends` chains too deeply");
                return None;
            }
            chain.push(parent);
            cur = parent;
        }
        let mut defs: HashMap<&'s str, Vec<(usize, &'s [Node])>> = HashMap::new();
        let mut ok = true;
        for (level, &ti) in chain.iter().enumerate() {
            let Ok(ast) = &sources[ti].ast else {
                return None;
            };
            let mut found = Vec::new();
            collect_blocks(ast, &mut found);
            let mut seen: Vec<&str> = Vec::new();
            for (name, at, body) in found {
                if seen.contains(&name) {
                    self.error(ti, at, format!("block `{name}` is defined twice"));
                    ok = false;
                    continue;
                }
                seen.push(name);
                // A child's block must exist further up, or it would never
                // be rendered: most often a typo.
                let is_base = level + 1 == chain.len();
                if !is_base {
                    let mut above = Vec::new();
                    for &up in &chain[level + 1..] {
                        if let Ok(a) = &sources[up].ast {
                            collect_blocks(a, &mut above);
                        }
                    }
                    if !above.iter().any(|(n, _, _)| *n == name) {
                        let names = above.iter().map(|(n, _, _)| *n);
                        let hint = suggest(name, names);
                        let base = &sources[chain[chain.len() - 1]].name;
                        self.error(ti, at, format!("block `{name}` is not in `{base}`{hint}"));
                        ok = false;
                        continue;
                    }
                }
                defs.entry(name).or_default().push((ti, body));
            }
        }
        ok.then(|| {
            (
                cur,
                Frame {
                    defs,
                    active: Vec::new(),
                },
            )
        })
    }

    /// The `extends` of template `t`, after checking that it comes first and
    /// that everything else at the top is a block. `None` inside the result
    /// means the template extends nothing; `None` outside means an error.
    #[allow(clippy::option_option)]
    fn extends_of(&mut self, t: usize, ast: &'s [Node]) -> Option<Option<(&'s str, usize)>> {
        let src = &self.sources[t].src;
        let blank = |n: &Node| matches!(n, Node::Text(r) if src[r.clone()].trim().is_empty());
        let mut first = None;
        for (i, n) in ast.iter().enumerate() {
            if let Node::Extends { name, at } = n {
                if ast[..i].iter().all(blank) && first.is_none() {
                    first = Some((name.as_str(), *at));
                } else {
                    self.error(
                        t,
                        *at,
                        "`extends` must be the first statement of a template",
                    );
                    return None;
                }
            }
        }
        if first.is_some() {
            for n in ast {
                match n {
                    Node::Extends { .. } | Node::Block { .. } => {}
                    n if blank(n) => {}
                    Node::Text(r) => {
                        self.error(t, r.start, "a template that extends another holds only blocks; this text would never be written");
                        return None;
                    }
                    _ => {
                        self.error(
                            t,
                            node_at(n).unwrap_or(0),
                            "a template that extends another holds only blocks; move this into one",
                        );
                        return None;
                    }
                }
            }
        }
        Some(first)
    }

    fn nodes(
        &mut self,
        t: usize,
        nodes: &'s [Node],
        mut ctx: Ctx,
        scope: &mut Scope,
    ) -> (Vec<CNode>, Ctx) {
        let mut out: Vec<CNode> = Vec::new();
        for node in nodes {
            self.nodes += 1;
            if self.nodes > MAX_NODES {
                break;
            }
            match node {
                Node::Text(r) => {
                    let text = &self.sources[t].src[r.clone()];
                    ctx.feed(text.as_bytes());
                    if let Some(CNode::Lit(prev)) = out.last_mut() {
                        let mut s = String::with_capacity(prev.len() + text.len());
                        s.push_str(prev);
                        s.push_str(text);
                        *prev = s.into_boxed_str();
                    } else {
                        out.push(CNode::Lit(text.into()));
                    }
                }
                Node::Out(e) => {
                    if let Some(n) = self.output(t, e, ctx, scope) {
                        out.push(n);
                    }
                    ctx = ctx.after_output();
                }
                Node::If { arms, otherwise } => {
                    let start = ctx;
                    let mut compiled = Vec::with_capacity(arms.len());
                    let mut ends = Vec::with_capacity(arms.len() + 1);
                    for (cond, body) in arms {
                        let c = self.cond(t, cond, scope);
                        let (b, end) = self.nodes(t, body, start, scope);
                        compiled.push((c, b));
                        ends.push(end);
                    }
                    let other = match otherwise {
                        Some(body) => {
                            let (b, end) = self.nodes(t, body, start, scope);
                            ends.push(end);
                            b
                        }
                        None => {
                            ends.push(start);
                            Vec::new()
                        }
                    };
                    let at = arms[0].0.at;
                    ctx = self.join_all(t, at, start, &ends, "branches of this `if`");
                    out.push(CNode::If(compiled, other));
                }
                Node::For {
                    var,
                    var_at,
                    list,
                    body,
                    otherwise,
                } => {
                    let (lexpr, lty) = self.expr(t, list, scope);
                    let item = match lty {
                        Ty::List(k) => Ty::of(&k),
                        Ty::Unknown => Ty::Unknown,
                        other => {
                            self.error(
                                t,
                                list.at,
                                format!("`for` needs a list, and this is {}", other.describe()),
                            );
                            Ty::Unknown
                        }
                    };
                    if var == &scope.root_name {
                        self.error(
                            t,
                            *var_at,
                            format!("`{var}` already names the template's root value"),
                        );
                    }
                    let slot = u16::try_from(scope.next_slot()).unwrap_or(u16::MAX);
                    scope.vars.push(Var {
                        name: var.clone(),
                        slot,
                        item,
                    });
                    self.slots = self.slots.max(scope.next_slot());
                    // A second pass of the body starts where the first ended,
                    // so probe the body first and compile it from the join.
                    let start = ctx;
                    self.quiet += 1;
                    let mark = self.nodes;
                    let (_, probe_end) = self.nodes(t, body, start, scope);
                    self.nodes = mark;
                    self.quiet -= 1;
                    let entry = match start.join(probe_end) {
                        Some(j) => j,
                        None => {
                            self.error(
                                t,
                                list.at,
                                format!(
                                    "the body of this `for` starts in {} but ends in {}",
                                    start.describe(),
                                    probe_end.describe()
                                ),
                            );
                            start
                        }
                    };
                    let (cbody, end) = self.nodes(t, body, entry, scope);
                    let _ = scope.vars.pop();
                    let (cother, other_end) = match otherwise {
                        Some(b) => self.nodes(t, b, start, scope),
                        None => (Vec::new(), start),
                    };
                    ctx = self.join_all(
                        t,
                        list.at,
                        start,
                        &[entry, end, other_end],
                        "body and `else` of this `for`",
                    );
                    out.push(CNode::For {
                        list: lexpr,
                        slot,
                        body: cbody,
                        otherwise: cother,
                    });
                }
                Node::Include { name, at, with } => {
                    let Some(&inc) = self.index.get(name.as_str()) else {
                        let mut names: Vec<&str> = self.index.keys().copied().collect();
                        names.sort_unstable();
                        let hint = suggest(name, names.iter().copied());
                        self.error(t, *at, format!("unknown template `{name}`{hint}"));
                        continue;
                    };
                    if inc == t || self.stack.iter().any(|&(x, _)| x == inc) {
                        self.error(t, *at, format!("`{name}` includes itself"));
                        continue;
                    }
                    if self.stack.len() >= MAX_INCLUDE_DEPTH {
                        self.error(t, *at, "includes nest too deeply");
                        continue;
                    }
                    if self.sources[inc].ast.is_err() {
                        self.error(t, *at, format!("`{name}` has errors of its own"));
                        continue;
                    }
                    let bound = match with {
                        None => None,
                        Some(e) => {
                            let (value, ty) = self.expr(t, e, scope);
                            match ty {
                                Ty::Object(schema) => Some((value, schema)),
                                Ty::Unknown => continue,
                                other => {
                                    self.error(
                                        t,
                                        e.at,
                                        format!(
                                            "`include ... with` needs an object, and this is {}",
                                            other.describe()
                                        ),
                                    );
                                    continue;
                                }
                            }
                        }
                    };
                    self.stack.push((t, *at));
                    let Some((root, frame)) = self.layout(inc) else {
                        let _ = self.stack.pop();
                        continue;
                    };
                    let Ok(ast) = &self.sources[root].ast else {
                        let _ = self.stack.pop();
                        continue;
                    };
                    self.frames.push(frame);
                    let (inner, end) = match bound {
                        None => self.nodes(root, ast, ctx, scope),
                        Some((value, schema)) => {
                            let slot = scope.next_slot();
                            let mut sub = Scope {
                                root_name: self.sources[inc].schema.name().to_owned(),
                                root: schema,
                                root_slot: Some(u16::try_from(slot).unwrap_or(u16::MAX)),
                                base: slot + 1,
                                vars: Vec::new(),
                            };
                            self.slots = self.slots.max(slot + 1);
                            let (body, end) = self.nodes(root, ast, ctx, &mut sub);
                            let slot = u16::try_from(slot).unwrap_or(u16::MAX);
                            (vec![CNode::Bind { slot, value, body }], end)
                        }
                    };
                    let _ = self.frames.pop();
                    let _ = self.stack.pop();
                    ctx = end;
                    for n in inner {
                        match (out.last_mut(), n) {
                            (Some(CNode::Lit(prev)), CNode::Lit(s)) => {
                                let mut joined = String::with_capacity(prev.len() + s.len());
                                joined.push_str(prev);
                                joined.push_str(&s);
                                *prev = joined.into_boxed_str();
                            }
                            (_, n) => out.push(n),
                        }
                    }
                }
                Node::Extends { at, .. } => {
                    self.error(
                        t,
                        *at,
                        "`extends` must be the first statement of a template",
                    );
                }
                Node::Block { name, at, .. } => {
                    let Some(frame) = self.frames.last_mut() else {
                        continue;
                    };
                    if frame.active.iter().any(|(n, _)| *n == name.as_str()) {
                        self.error(t, *at, format!("block `{name}` ends up inside itself"));
                        continue;
                    }
                    let Some(&(dt, body)) = frame.defs.get(name.as_str()).and_then(|d| d.first())
                    else {
                        continue;
                    };
                    let key: &'s str = frame
                        .defs
                        .get_key_value(name.as_str())
                        .map_or("", |(k, _)| k);
                    frame.active.push((key, 0));
                    let (inner, end) = self.nodes(dt, body, ctx, scope);
                    if let Some(f) = self.frames.last_mut() {
                        let _ = f.active.pop();
                    }
                    ctx = end;
                    out.extend(inner);
                }
                Node::Super { at } => {
                    let Some(frame) = self.frames.last_mut() else {
                        continue;
                    };
                    let Some(&(name, level)) = frame.active.last() else {
                        self.error(t, *at, "`super()` is only available inside a block");
                        continue;
                    };
                    let Some(&(dt, body)) = frame.defs.get(name).and_then(|d| d.get(level + 1))
                    else {
                        self.error(
                            t,
                            *at,
                            format!("block `{name}` has no parent version for `super()`"),
                        );
                        continue;
                    };
                    frame.active.push((name, level + 1));
                    let (inner, end) = self.nodes(dt, body, ctx, scope);
                    if let Some(f) = self.frames.last_mut() {
                        let _ = f.active.pop();
                    }
                    ctx = end;
                    out.extend(inner);
                }
                Node::Try { body, fallback, at } => {
                    let start = ctx;
                    let (b, e1) = self.nodes(t, body, start, scope);
                    let (f, e2) = self.nodes(t, fallback, start, scope);
                    ctx =
                        self.join_all(t, *at, start, &[e1, e2], "body and fallback of this `try`");
                    out.push(CNode::Try(b, f));
                }
            }
        }
        (out, ctx)
    }

    fn join_all(&mut self, t: usize, at: usize, start: Ctx, ends: &[Ctx], what: &str) -> Ctx {
        let mut j = ends[0];
        for &e in &ends[1..] {
            match j.join(e) {
                Some(x) => j = x,
                None => {
                    self.error(
                        t,
                        at,
                        format!(
                            "the {what} end in different places: {} and {}",
                            j.describe(),
                            e.describe()
                        ),
                    );
                    return start;
                }
            }
        }
        j
    }

    fn output(&mut self, t: usize, e: &Expr, ctx: Ctx, scope: &Scope) -> Option<CNode> {
        let (inner, raw) = match &e.kind {
            ExprKind::Filter { base, name, args } if name == "raw" => {
                if !args.is_empty() {
                    self.error(t, e.at, "`raw` takes no arguments");
                }
                (&**base, true)
            }
            _ => (e, false),
        };
        let slot = ctx.slot();
        let (ce, ty) = self.expr(t, inner, scope);
        let esc = match slot {
            Slot::Refused(why) => {
                self.error(t, e.at, why);
                return None;
            }
            Slot::Text => Esc::Text,
            Slot::Attr => Esc::Attr,
            Slot::Url(UrlPart::Start) => Esc::UrlStart,
            Slot::Url(UrlPart::Path) => Esc::UrlPath,
            Slot::Url(UrlPart::Query) => Esc::UrlQuery,
            Slot::Js { string, attr } => Esc::Js { string, attr },
            Slot::Css { string, attr } => Esc::Css { string, attr },
        };
        match ty {
            Ty::Object(s) if s.renders() => {
                if raw {
                    self.error(t, e.at, "`raw` applies to text, not to a block");
                } else if esc != Esc::Text {
                    self.error(
                        t,
                        e.at,
                        format!(
                            "a block renders markup, so it can only go in HTML text, not in {}",
                            ctx.describe()
                        ),
                    );
                }
                return Some(CNode::Block(ce));
            }
            Ty::Object(s) => {
                let fields: Vec<&str> = s.names().take(6).collect();
                self.error(
                    t,
                    e.at,
                    format!(
                        "this is an object of `{}`; write one of its fields ({})",
                        s.name(),
                        fields.join(", ")
                    ),
                );
                return None;
            }
            Ty::List(_) => {
                self.error(
                    t,
                    e.at,
                    "this is a list; loop over it with `for`, or write `| length`",
                );
                return None;
            }
            _ => {}
        }
        if raw {
            if esc != Esc::Text {
                self.error(
                    t,
                    e.at,
                    format!(
                        "`raw` is allowed in HTML text only, not in {}",
                        ctx.describe()
                    ),
                );
                return None;
            }
            if !matches!(ty, Ty::Text | Ty::Html | Ty::Unknown) {
                self.error(t, e.at, "`raw` applies to text");
            }
            return Some(CNode::Out(ce, Esc::Raw));
        }
        Some(CNode::Out(ce, esc))
    }

    fn cond(&mut self, t: usize, e: &Expr, scope: &Scope) -> CExpr {
        self.expr(t, e, scope).0
    }

    fn expr(&mut self, t: usize, e: &Expr, scope: &Scope) -> (CExpr, Ty) {
        match &e.kind {
            ExprKind::Str(s) => (CExpr::Str(s.as_str().into()), Ty::Text),
            ExprKind::Int(n) => (CExpr::Int(*n), Ty::Int),
            ExprKind::Float(f) => (CExpr::Float(*f), Ty::Float),
            ExprKind::Bool(b) => (CExpr::Bool(*b), Ty::Bool),
            ExprKind::Not(x) => {
                let (c, _) = self.expr(t, x, scope);
                (CExpr::Not(Box::new(c)), Ty::Bool)
            }
            ExprKind::And(a, b) => {
                let (ca, _) = self.expr(t, a, scope);
                let (cb, _) = self.expr(t, b, scope);
                (CExpr::And(Box::new(ca), Box::new(cb)), Ty::Bool)
            }
            ExprKind::Or(a, b) => {
                let (ca, _) = self.expr(t, a, scope);
                let (cb, _) = self.expr(t, b, scope);
                (CExpr::Or(Box::new(ca), Box::new(cb)), Ty::Bool)
            }
            ExprKind::Cmp(op, a, b) => {
                let (ca, ta) = self.expr(t, a, scope);
                let (cb, tb) = self.expr(t, b, scope);
                let ok = match (&ta, &tb) {
                    (Ty::Unknown, _) | (_, Ty::Unknown) => true,
                    (x, y) if x.is_number() && y.is_number() => true,
                    (x, y) if x.is_text() && y.is_text() => true,
                    (Ty::Bool, Ty::Bool) => matches!(op, CmpOp::Eq | CmpOp::Ne),
                    _ => false,
                };
                if !ok {
                    self.error(
                        t,
                        e.at,
                        format!("cannot compare {} with {}", ta.describe(), tb.describe()),
                    );
                }
                (CExpr::Cmp(*op, Box::new(ca), Box::new(cb)), Ty::Bool)
            }
            ExprKind::Filter { base, name, args } => self.filter(t, e, base, name, args, scope),
            ExprKind::Path(segs) => self.path(t, segs, scope),
        }
    }

    fn filter(
        &mut self,
        t: usize,
        e: &Expr,
        base: &Expr,
        name: &str,
        args: &[Expr],
        scope: &Scope,
    ) -> (CExpr, Ty) {
        let (cb, tb) = self.expr(t, base, scope);
        let want = |n: usize| args.len() == n;
        let arity = match name {
            "default" => 1,
            "upper" | "lower" | "trim" | "length" | "raw" => 0,
            _ => {
                let hint = suggest(name, FILTERS.split(", "));
                self.error(
                    t,
                    e.at,
                    format!("unknown filter `{name}`{hint}; the filters are {FILTERS}"),
                );
                return (cb, Ty::Unknown);
            }
        };
        if !want(arity) {
            self.error(
                t,
                e.at,
                format!(
                    "`{name}` takes {arity} argument{}",
                    if arity == 1 { "" } else { "s" }
                ),
            );
            return (cb, Ty::Unknown);
        }
        match name {
            "default" => {
                let (ca, ta) = self.expr(t, &args[0], scope);
                let ty = match (&tb, &ta) {
                    (Ty::Unknown, _) => ta,
                    (Ty::Object(_) | Ty::List(_), _) | (_, Ty::Object(_) | Ty::List(_)) => {
                        self.error(t, e.at, "`default` applies to text, numbers, and bools");
                        Ty::Unknown
                    }
                    // Mixed kinds are written as text.
                    (Ty::Html, Ty::Html) => Ty::Html,
                    (x, y) if std::mem::discriminant(x) == std::mem::discriminant(y) => tb,
                    _ => Ty::Text,
                };
                (CExpr::Default(Box::new(cb), Box::new(ca)), ty)
            }
            "upper" | "lower" | "trim" => {
                if !matches!(tb, Ty::Text | Ty::Unknown) {
                    self.error(
                        t,
                        e.at,
                        format!("`{name}` applies to text, and this is {}", tb.describe()),
                    );
                }
                let f = match name {
                    "upper" => Filt::Upper,
                    "lower" => Filt::Lower,
                    _ => Filt::Trim,
                };
                (CExpr::Filter(f, Box::new(cb)), Ty::Text)
            }
            "length" => {
                if !matches!(tb, Ty::Text | Ty::Html | Ty::List(_) | Ty::Unknown) {
                    self.error(
                        t,
                        e.at,
                        format!(
                            "`length` applies to text and lists, and this is {}",
                            tb.describe()
                        ),
                    );
                }
                (CExpr::Filter(Filt::Length, Box::new(cb)), Ty::Int)
            }
            _ => {
                self.error(t, e.at, "`raw` must be the last filter of an output");
                (cb, Ty::Unknown)
            }
        }
    }

    fn path(&mut self, t: usize, segs: &[(String, usize)], scope: &Scope) -> (CExpr, Ty) {
        let (first, at) = (&segs[0].0, segs[0].1);
        let (mut ty, base): (Ty, Option<u16>) =
            if let Some(v) = scope.vars.iter().rev().find(|v| &v.name == first) {
                (v.item.clone(), Some(v.slot))
            } else if first == "loop" {
                let Some(v) = scope.vars.last() else {
                    self.error(t, at, "`loop` is only available inside a `for`");
                    return (CExpr::Bool(false), Ty::Unknown);
                };
                let attr = match segs.get(1).map(|s| s.0.as_str()) {
                    Some("index") => (LoopAttr::Index, Ty::Int),
                    Some("index0") => (LoopAttr::Index0, Ty::Int),
                    Some("revindex") => (LoopAttr::RevIndex, Ty::Int),
                    Some("first") => (LoopAttr::First, Ty::Bool),
                    Some("last") => (LoopAttr::Last, Ty::Bool),
                    Some("length") => (LoopAttr::Length, Ty::Int),
                    _ => {
                        self.error(
                            t,
                            segs.get(1).map_or(at, |s| s.1),
                            "`loop` has index, index0, revindex, first, last, and length",
                        );
                        return (CExpr::Bool(false), Ty::Unknown);
                    }
                };
                if let Some(extra) = segs.get(2) {
                    self.error(t, extra.1, format!("`loop.{}` has no fields", segs[1].0));
                }
                return (CExpr::Loop(v.slot, attr.0), attr.1);
            } else if first == &scope.root_name && scope.root_slot.is_some() {
                (Ty::Object(scope.root.clone()), scope.root_slot)
            } else if first == &scope.root_name {
                (Ty::Object(scope.root.clone()), None)
            } else {
                let mut names: Vec<&str> = scope.vars.iter().map(|v| v.name.as_str()).collect();
                names.push(&scope.root_name);
                let hint = suggest(first, names.iter().copied());
                let known = names
                    .iter()
                    .map(|n| format!("`{n}`"))
                    .collect::<Vec<_>>()
                    .join(", ");
                self.error(
                    t,
                    at,
                    format!("unknown name `{first}`{hint}; names here: {known}"),
                );
                return (CExpr::Bool(false), Ty::Unknown);
            };
        let mut ids = Vec::with_capacity(segs.len() - 1);
        let mut prev = first.as_str();
        for (name, sat) in &segs[1..] {
            match &ty {
                Ty::Object(s) => match s.field(name) {
                    Some((id, kind)) => {
                        ids.push(id);
                        ty = Ty::of(kind);
                    }
                    None => {
                        let hint = suggest(name, s.names());
                        self.error(
                            t,
                            *sat,
                            format!("`{prev}` ({}) has no field `{name}`{hint}", s.name()),
                        );
                        return (CExpr::Bool(false), Ty::Unknown);
                    }
                },
                Ty::Unknown => return (CExpr::Bool(false), Ty::Unknown),
                other => {
                    self.error(
                        t,
                        *sat,
                        format!("`{prev}` is {} and has no fields", other.describe()),
                    );
                    return (CExpr::Bool(false), Ty::Unknown);
                }
            }
            prev = name;
        }
        let ids = ids.into_boxed_slice();
        let ce = match base {
            Some(slot) => CExpr::Slot(slot, ids),
            None => CExpr::Root(ids),
        };
        (ce, ty)
    }
}

/// ", did you mean `x`?" for the closest name within two edits, or "".
fn suggest<'a>(name: &str, options: impl Iterator<Item = &'a str>) -> String {
    let mut best: Option<(usize, &str)> = None;
    for o in options {
        let d = distance(name, o);
        if d <= 2 && d < name.len().max(1) && best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, o));
        }
    }
    best.map_or_else(String::new, |(_, o)| format!(", did you mean `{o}`?"))
}

fn distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.len().abs_diff(b.len()) > 2 || a.len() > 64 || b.len() > 64 {
        return usize::MAX;
    }
    let mut before: Vec<usize> = vec![0; b.len() + 1];
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for i in 1..=a.len() {
        cur[0] = i;
        for j in 1..=b.len() {
            let sub = prev[j - 1] + usize::from(a[i - 1] != b[j - 1]);
            cur[j] = sub.min(prev[j] + 1).min(cur[j - 1] + 1);
            // A swap of two neighbours counts as one edit.
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                cur[j] = cur[j].min(before[j - 2] + 1);
            }
        }
        std::mem::swap(&mut before, &mut prev);
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// Compiles every source. Returns the programs (in source order) or every
/// diagnostic.
pub(crate) fn compile_all(sources: &[Source]) -> Result<Vec<Program>, Vec<Diag>> {
    let mut diags = Vec::new();
    let mut index: HashMap<&str, usize> = HashMap::with_capacity(sources.len());
    for (i, s) in sources.iter().enumerate() {
        if index.insert(&s.name, i).is_some() {
            diags.push(Diag {
                template: i,
                at: 0,
                msg: format!("template `{}` is defined twice", s.name),
                from: None,
            });
        }
        if let Err(e) = &s.ast {
            diags.push(Diag {
                template: i,
                at: e.at,
                msg: e.msg.clone(),
                from: None,
            });
        }
    }
    let mut c = Compiler {
        sources,
        index: &index,
        diags,
        quiet: 0,
        stack: Vec::new(),
        nodes: 0,
        slots: 0,
        frames: Vec::new(),
    };
    let mut programs = Vec::with_capacity(sources.len());
    for t in 0..sources.len() {
        if let Some(p) = c.template(t) {
            programs.push(p);
        }
    }
    let mut diags = c.diags;
    if diags.is_empty() && programs.len() == sources.len() {
        return Ok(programs);
    }
    // An included template's error is found once per place it is included
    // from and once on its own: report each one once.
    let mut seen = Vec::new();
    diags.retain(|d| {
        let key = (d.template, d.at, d.msg.clone());
        if seen.contains(&key) {
            false
        } else {
            seen.push(key);
            true
        }
    });
    Err(diags)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggestions() {
        assert_eq!(
            suggest("titel", ["title", "body"].into_iter()),
            ", did you mean `title`?"
        );
        assert_eq!(suggest("x", ["title"].into_iter()), "");
        assert_eq!(distance("kitten", "sitting"), 3);
        assert_eq!(distance("a", "abcd"), usize::MAX);
        assert_eq!(distance("abc", "abd"), 1);
        assert_eq!(distance("nva", "nav"), 1);
        assert_eq!(distance("ab", "ba"), 1);
    }
}
