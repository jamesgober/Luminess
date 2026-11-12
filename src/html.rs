//! Tracks where in an HTML document the template's literal text leaves the
//! output, so each `{{ }}` gets the escaper for that place. This is a small
//! subset of the HTML tokenizer: enough to tell text from tags, attribute
//! values of each kind, comments, and raw-text elements. Scripts and styles
//! are followed by the trackers in `script`.

use crate::script::{Css, Js, JsMode};

/// The kind of an attribute value, from the attribute's name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AttrKind {
    /// Plain text: `title`, `alt`, `class`, `data-*`, ...
    Text,
    /// A URL: `href`, `src`, `action`, ...
    Url,
    /// JavaScript: `onclick` and every other `on*` handler.
    Script,
    /// CSS: `style`.
    Style,
    /// HTML parsed again after entity decoding: `srcdoc`. Never written to.
    Html,
    /// The value of an SVG animation (`values`, `to`, `from`, `by` on
    /// `animate`, `set`, and their kin), which may set any attribute,
    /// an `href` too. Never written to.
    Animation,
}

/// How an output in a URL attribute value is escaped, from the least strict
/// to the most.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum UrlPart {
    /// After the scheme, before `?` or `#`: normalized.
    Path,
    /// Nothing but whitespace written yet: the scheme is checked.
    Start,
    /// The scheme may still be open after text already written: normalized,
    /// and `:` encoded, so the value cannot finish a scheme.
    Open,
    /// After `?` or `#`: every reserved character is encoded.
    Query,
}

/// Where in a URL attribute value the template is.
///
/// A browser takes the scheme from the text before the first `:`, after
/// stripping leading whitespace and control characters and ignoring tabs
/// and line breaks. So the tracker follows whether that part may still be
/// open, and whether an output was written while it was: a value written
/// there may supply the letters of a scheme, and a literal `:` (or a
/// character reference, which may be one) after it would complete it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Url {
    pub part: UrlPart,
    /// The scheme may still be open: no `:`, `/`, `?`, or `#` yet.
    open: bool,
    /// An output was written while the scheme was open.
    valued: bool,
    /// A literal `:` or `&` came after such an output: a compile error.
    broken: bool,
}

impl Url {
    const START: Self = Self {
        part: UrlPart::Start,
        open: true,
        valued: false,
        broken: false,
    };

    fn step(mut self, c: u8) -> Self {
        if self.broken {
            return self;
        }
        if !self.open {
            if matches!(c, b'?' | b'#') {
                self.part = UrlPart::Query;
            }
            return self;
        }
        match c {
            b'?' | b'#' => {
                self.open = false;
                self.part = UrlPart::Query;
            }
            b':' | b'&' if self.valued => self.broken = true,
            b':' | b'/' => {
                self.open = false;
                if self.part != UrlPart::Query {
                    self.part = UrlPart::Path;
                }
            }
            // What a browser strips before the scheme.
            0..=0x20 => {}
            _ => {
                if self.part == UrlPart::Start {
                    self.part = UrlPart::Open;
                }
            }
        }
        self
    }

    fn after_output(mut self) -> Self {
        if self.open {
            self.valued = true;
            if self.part == UrlPart::Start {
                self.part = UrlPart::Open;
            }
        }
        self
    }

    /// Where two branches meet: what is safe for both.
    fn join(self, other: Self) -> Self {
        Self {
            part: self.part.max(other.part),
            open: self.open || other.open,
            valued: self.valued || other.valued,
            broken: self.broken || other.broken,
        }
    }
}

/// What an attribute value or raw-text element holds, as far as read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Sub {
    None,
    Url(Url),
    Js(Js),
    Css(Css),
    /// A `&` in an `on*` value: a character reference would change what the
    /// script means after decoding, so the next byte decides.
    JsAmp(Js),
    /// The same for a `style` value.
    CssAmp(Css),
}

/// An element whose content is not parsed as HTML: raw text and RCDATA, as
/// the HTML parser reads them (`noscript` as with scripting on).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RawElem {
    Script,
    Style,
    Title,
    Textarea,
    Xmp,
    Iframe,
    Noembed,
    Noframes,
    Noscript,
    /// Raw text to the end of the document.
    Plaintext,
}

impl RawElem {
    const ALL: [Self; 10] = [
        Self::Script,
        Self::Style,
        Self::Title,
        Self::Textarea,
        Self::Xmp,
        Self::Iframe,
        Self::Noembed,
        Self::Noframes,
        Self::Noscript,
        Self::Plaintext,
    ];

    fn name(self) -> &'static [u8] {
        match self {
            Self::Script => b"script",
            Self::Style => b"style",
            Self::Title => b"title",
            Self::Textarea => b"textarea",
            Self::Xmp => b"xmp",
            Self::Iframe => b"iframe",
            Self::Noembed => b"noembed",
            Self::Noframes => b"noframes",
            Self::Noscript => b"noscript",
            Self::Plaintext => b"plaintext",
        }
    }
}

const NAME_CAP: usize = 16;

/// A short lowercase name buffer: tag and attribute names longer than this
/// are kept as their first 16 bytes, which is enough to classify them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Name {
    buf: [u8; NAME_CAP],
    len: u8,
    long: bool,
}

impl Name {
    const EMPTY: Self = Self {
        buf: [0; NAME_CAP],
        len: 0,
        long: false,
    };

    fn of(c: u8) -> Self {
        let mut n = Self::EMPTY;
        n.push(c);
        n
    }

    fn push(&mut self, c: u8) {
        if usize::from(self.len) < NAME_CAP {
            self.buf[usize::from(self.len)] = c.to_ascii_lowercase();
            self.len += 1;
        } else {
            self.long = true;
        }
    }

    fn get(&self) -> &[u8] {
        &self.buf[..usize::from(self.len)]
    }

    fn is(&self, s: &[u8]) -> bool {
        !self.long && self.get() == s
    }
}

/// The tracker's state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Ctx {
    /// HTML text.
    Text,
    /// After `<`.
    Lt,
    /// Reading a start tag's name.
    TagName { name: Name },
    /// After `</`, reading an end tag's name up to `>`.
    EndTag,
    /// Inside a start tag, between attributes.
    Tag { tag: Name },
    /// Reading an attribute name.
    AttrName { tag: Name, attr: Name },
    /// After an attribute name, before `=` or the next attribute.
    AfterAttrName { tag: Name, attr: Name },
    /// After `=`.
    BeforeValue { tag: Name, attr: Name },
    /// Inside a quoted value.
    Value {
        tag: Name,
        quote: u8,
        kind: AttrKind,
        sub: Sub,
    },
    /// Inside an unquoted value.
    Unquoted { tag: Name },
    /// After `<!`; `dashes` counts the `-` read so far (two open a comment).
    Bang { dashes: u8 },
    /// Inside `<!...>` or `<?...>`.
    Decl,
    /// Inside `<!-- -->`, in one of the HTML tokenizer's comment states.
    Comment { st: CommentSt },
    /// Inside a raw-text element; `matched` counts the bytes of `</name` read.
    Raw {
        elem: RawElem,
        matched: u8,
        sub: Sub,
        /// For a script: the HTML parser's escaped states.
        script: ScriptData,
    },
}

/// The HTML parser's view of a script's content. After `<!--` it is
/// "escaped"; a `<script` there makes it "double escaped", where
/// `</script>` does not end the element, until `-->`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ScriptData {
    /// 0: plain, 1: escaped, 2: double escaped.
    state: u8,
    /// The last bytes read that may still complete a pattern, lowercase,
    /// newest last, zeros before them.
    recent: [u8; 8],
}

impl ScriptData {
    const START: Self = Self {
        state: 0,
        recent: [0; 8],
    };

    /// `true` when the bytes read, then `c`, end with `pat`.
    fn ends(&self, c: u8, pat: &[u8]) -> bool {
        let n = pat.len();
        pat[n - 1] == c && self.recent[8 - (n - 1)..] == pat[..n - 1]
    }

    fn step(mut self, c: u8) -> Self {
        let c = c.to_ascii_lowercase();
        let delim = is_space(c) || c == b'/' || c == b'>';
        self.state = match self.state {
            0 if self.ends(c, b"<!--") => 1,
            1 | 2 if self.ends(c, b"-->") => 0,
            1 if delim && self.recent[1..] == *b"<script" => 2,
            2 if delim && self.recent == *b"</script" => 1,
            s => s,
        };
        self.recent.rotate_left(1);
        self.recent[7] = c;
        // Keep only what may still complete a pattern, so that two states
        // that read what follows the same way are equal: a loop body then
        // ends where it started, and is compiled once.
        let k = self.pending().len();
        self.recent[..8 - k].fill(0);
        self
    }

    fn double(self) -> bool {
        self.state == 2
    }

    /// The bytes just read that begin a pattern this state looks for: what
    /// a following byte could complete.
    fn pending(&self) -> &[u8] {
        let pats: &[&[u8]] = match self.state {
            0 => &[b"<!--"],
            1 => &[b"-->", b"<script"],
            _ => &[b"-->", b"</script"],
        };
        let mut k = 0;
        for pat in pats {
            // The delimiter after `<script` is not part of the pattern, so
            // all of it may be pending.
            let most = if pat.ends_with(b"t") {
                pat.len()
            } else {
                pat.len() - 1
            };
            for n in (k + 1..=most).rev() {
                if self.recent[8 - n..] == pat[..n] {
                    k = n;
                    break;
                }
            }
        }
        &self.recent[8 - k..]
    }

    /// Where two branches meet: the same state, and the same part of a
    /// pattern pending, so what follows reads the same after either.
    fn join(self, other: Self) -> Option<Self> {
        if self == other {
            return Some(self);
        }
        let pending = self.pending();
        if self.state != other.state || pending != other.pending() {
            return None;
        }
        let mut recent = [0; 8];
        recent[8 - pending.len()..].copy_from_slice(pending);
        Some(Self {
            state: self.state,
            recent,
        })
    }
}

/// The HTML tokenizer's comment states: a comment ends at `-->`, at `--!>`,
/// and, right after it opens, at `>` or `->` (`<!-->`, `<!--->`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommentSt {
    /// Right after `<!--`.
    Start,
    /// After `<!---`.
    StartDash,
    Body,
    /// After one `-`.
    EndDash,
    /// After `--`.
    End,
    /// After `--!`.
    EndBang,
}

/// What a `{{ }}` at this place writes, or why it cannot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Slot {
    /// HTML text: escape, and trusted markup passes.
    Text,
    /// A quoted attribute value, or text in `<title>` and `<textarea>`.
    Attr,
    /// A URL attribute value.
    Url(UrlPart),
    /// JavaScript: a whole literal in code, or string content; `attr` when
    /// inside an `on*` attribute.
    Js { string: bool, attr: bool },
    /// CSS: a value in code, or string content.
    Css { string: bool, attr: bool },
    /// Not a place output can go.
    Refused(&'static str),
}

fn raw_elem(tag: &Name) -> Option<RawElem> {
    RawElem::ALL.into_iter().find(|e| tag.is(e.name()))
}

fn attr_kind(tag: &Name, attr: &Name) -> AttrKind {
    let n = attr.get();
    let animates = [
        b"animate".as_slice(),
        b"set",
        b"animatemotion",
        b"animatetransform",
    ]
    .iter()
    .any(|t| tag.is(t));
    if animates && matches!(n, b"values" | b"to" | b"from" | b"by") && !attr.long {
        return AttrKind::Animation;
    }
    if attr.long {
        // A long name is classified by its prefix only.
        if n.starts_with(b"on") {
            return AttrKind::Script;
        }
        return AttrKind::Text;
    }
    match n {
        b"href" | b"src" | b"action" | b"formaction" | b"cite" | b"poster" | b"background"
        | b"longdesc" | b"usemap" | b"data" | b"manifest" | b"codebase" | b"icon" | b"profile"
        | b"srcset" | b"xlink:href" | b"ping" | b"archive" | b"classid" | b"lowsrc" | b"dynsrc" => {
            AttrKind::Url
        }
        b"style" => AttrKind::Style,
        b"srcdoc" => AttrKind::Html,
        _ if n.starts_with(b"on") => AttrKind::Script,
        _ if contains(n, b"url") || contains(n, b"uri") => AttrKind::Url,
        _ => AttrKind::Text,
    }
}

fn sub_for(kind: AttrKind) -> Sub {
    match kind {
        AttrKind::Url => Sub::Url(Url::START),
        AttrKind::Script => Sub::Js(Js::START),
        AttrKind::Style => Sub::Css(Css::Code),
        AttrKind::Text | AttrKind::Html | AttrKind::Animation => Sub::None,
    }
}

fn contains(h: &[u8], n: &[u8]) -> bool {
    h.windows(n.len()).any(|w| w == n)
}

fn is_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\r' | b'\x0c')
}

impl Sub {
    /// The sub-state after one byte of an attribute value (`attr`) or of a
    /// raw-text element's content.
    fn step(self, c: u8, attr: bool) -> Self {
        match self {
            Self::None => Self::None,
            Self::Url(url) => Self::Url(url.step(c)),
            Self::Js(js) => {
                if attr && c == b'&' {
                    Self::JsAmp(js)
                } else {
                    Self::Js(js.step(c))
                }
            }
            Self::Css(css) => {
                if attr && c == b'&' {
                    Self::CssAmp(css)
                } else {
                    Self::Css(css.step(c))
                }
            }
            Self::JsAmp(js) => {
                if c.is_ascii_alphanumeric() || c == b'#' {
                    Self::Js(Js::LOST)
                } else {
                    Self::Js(js.step(b'&')).step(c, attr)
                }
            }
            Self::CssAmp(css) => {
                if c.is_ascii_alphanumeric() || c == b'#' {
                    Self::Css(Css::Lost)
                } else {
                    Self::Css(css.step(b'&')).step(c, attr)
                }
            }
        }
    }

    fn join(self, other: Self) -> Option<Self> {
        if self == other {
            return Some(self);
        }
        match (self, other) {
            // Two places in one URL join to what is safe for both, so an
            // output after them is never under-escaped.
            (Self::Url(a), Self::Url(b)) => Some(Self::Url(a.join(b))),
            (Self::Js(a), Self::Js(b)) => a.join(b).map(Self::Js),
            // Right after an output is the stricter of the two.
            (Self::Css(Css::Code | Css::AfterValue), Self::Css(Css::Code | Css::AfterValue)) => {
                Some(Self::Css(Css::AfterValue))
            }
            _ => None,
        }
    }

    fn after_output(self) -> Self {
        match self {
            Self::Url(url) => Self::Url(url.after_output()),
            Self::Js(js) => Self::Js(js.after_value()),
            Self::Css(Css::Code) => Self::Css(Css::AfterValue),
            other => other,
        }
    }

    fn js_slot(js: Js, attr: bool) -> Slot {
        match js.mode {
            JsMode::Code => Slot::Js {
                string: false,
                attr,
            },
            JsMode::Str(_) | JsMode::Tmpl => Slot::Js { string: true, attr },
            JsMode::TmplDollar => Slot::Refused("output right after `$` in a template literal"),
            JsMode::Slash => {
                Slot::Refused("output right after `/` in JavaScript; add a space or parentheses")
            }
            JsMode::StrEsc(_) | JsMode::TmplEsc => {
                Slot::Refused("output right after a backslash in JavaScript")
            }
            JsMode::Regex | JsMode::RegexEsc | JsMode::RegexClass | JsMode::RegexClassEsc => {
                Slot::Refused("output inside a JavaScript regular expression")
            }
            JsMode::LineComment
            | JsMode::LineCommentE2
            | JsMode::LineCommentE280
            | JsMode::BlockComment
            | JsMode::BlockCommentStar => Slot::Refused("output inside a JavaScript comment"),
            JsMode::Lost => Slot::Refused(if attr {
                "output in an event-handler attribute that uses character references (`&...;`); move the script to a `<script>` element"
            } else {
                "output in JavaScript the compiler cannot follow"
            }),
        }
    }

    fn css_slot(css: Css, attr: bool) -> Slot {
        match css {
            Css::Code => Slot::Css {
                string: false,
                attr,
            },
            Css::Str(_) => Slot::Css { string: true, attr },
            Css::AfterValue | Css::Broken => {
                Slot::Refused("two CSS values side by side; put a space between them")
            }
            Css::Slash | Css::Esc | Css::StrEsc(_) => {
                Slot::Refused("output right after `/` or a backslash in CSS")
            }
            Css::Comment | Css::CommentStar => Slot::Refused("output inside a CSS comment"),
            Css::Lost => Slot::Refused(
                "output in a `style` attribute that uses character references (`&...;`); move the rules to a `<style>` element",
            ),
        }
    }
}

impl Ctx {
    /// The state after a start tag's `>`.
    fn after_tag(tag: Name) -> Self {
        match raw_elem(&tag) {
            Some(elem) => Self::Raw {
                elem,
                matched: 0,
                sub: match elem {
                    RawElem::Script => Sub::Js(Js::START),
                    RawElem::Style => Sub::Css(Css::Code),
                    _ => Sub::None,
                },
                script: ScriptData::START,
            },
            None => Self::Text,
        }
    }

    /// Feeds literal bytes.
    /// Feeds literal bytes, and returns the error they make, if any: a `:`
    /// or a character reference after an output, before a URL's scheme is
    /// closed. After an error the URL goes on as if in its query, so the
    /// error is reported once.
    pub(crate) fn feed(&mut self, bytes: &[u8]) -> Option<&'static str> {
        let mut error = None;
        for &c in bytes {
            *self = self.step(c);
            if let Some(e) = self.take_error() {
                error = Some(e);
            }
        }
        error
    }

    /// The state after one byte.
    fn step(self, c: u8) -> Self {
        match self {
            Self::Text => {
                if c == b'<' {
                    Self::Lt
                } else {
                    Self::Text
                }
            }
            Self::Lt => match c {
                b'/' => Self::EndTag,
                b'!' => Self::Bang { dashes: 0 },
                b'?' => Self::Decl,
                c if c.is_ascii_alphabetic() => Self::TagName { name: Name::of(c) },
                b'<' => Self::Lt,
                _ => Self::Text,
            },
            Self::TagName { mut name } => match c {
                b'>' => Self::after_tag(name),
                b'/' => Self::Tag { tag: name },
                c if is_space(c) => Self::Tag { tag: name },
                c => {
                    name.push(c);
                    Self::TagName { name }
                }
            },
            Self::EndTag => {
                if c == b'>' {
                    Self::Text
                } else {
                    Self::EndTag
                }
            }
            Self::Tag { tag } => match c {
                b'>' => Self::after_tag(tag),
                b'/' => Self::Tag { tag },
                c if is_space(c) => Self::Tag { tag },
                c => Self::AttrName {
                    tag,
                    attr: Name::of(c),
                },
            },
            Self::AttrName { tag, mut attr } => match c {
                b'>' => Self::after_tag(tag),
                b'=' => Self::BeforeValue { tag, attr },
                b'/' => Self::Tag { tag },
                c if is_space(c) => Self::AfterAttrName { tag, attr },
                c => {
                    attr.push(c);
                    Self::AttrName { tag, attr }
                }
            },
            Self::AfterAttrName { tag, attr } => match c {
                b'>' => Self::after_tag(tag),
                b'=' => Self::BeforeValue { tag, attr },
                b'/' => Self::Tag { tag },
                c if is_space(c) => Self::AfterAttrName { tag, attr },
                c => Self::AttrName {
                    tag,
                    attr: Name::of(c),
                },
            },
            Self::BeforeValue { tag, attr } => match c {
                b'"' | b'\'' => {
                    let kind = attr_kind(&tag, &attr);
                    Self::Value {
                        tag,
                        quote: c,
                        kind,
                        sub: sub_for(kind),
                    }
                }
                b'>' => Self::after_tag(tag),
                c if is_space(c) => Self::BeforeValue { tag, attr },
                _ => Self::Unquoted { tag },
            },
            Self::Value {
                tag,
                quote,
                kind,
                sub,
            } => {
                if c == quote {
                    Self::Tag { tag }
                } else {
                    Self::Value {
                        tag,
                        quote,
                        kind,
                        sub: sub.step(c, true),
                    }
                }
            }
            Self::Unquoted { tag } => match c {
                b'>' => Self::after_tag(tag),
                c if is_space(c) => Self::Tag { tag },
                _ => Self::Unquoted { tag },
            },
            Self::Bang { dashes } => match (dashes, c) {
                (0, b'-') => Self::Bang { dashes: 1 },
                (1, b'-') => Self::Comment {
                    st: CommentSt::Start,
                },
                (_, b'>') => Self::Text,
                _ => Self::Decl,
            },
            Self::Decl => {
                if c == b'>' {
                    Self::Text
                } else {
                    Self::Decl
                }
            }
            Self::Comment { st } => {
                use CommentSt as S;
                let st = match (st, c) {
                    (S::Start | S::StartDash | S::End | S::EndBang, b'>') => return Self::Text,
                    (S::Start, b'-') => S::StartDash,
                    (S::StartDash | S::EndDash | S::End, b'-') => S::End,
                    (S::Body | S::EndBang, b'-') => S::EndDash,
                    (S::End, b'!') => S::EndBang,
                    _ => S::Body,
                };
                Self::Comment { st }
            }
            Self::Raw {
                elem: RawElem::Plaintext,
                ..
            } => self,
            Self::Raw {
                elem,
                matched,
                sub,
                script,
            } => {
                // The element ends at `</name` whatever the script or style
                // inside it is doing: that is how the HTML parser reads it,
                // except in a script's double-escaped state.
                let name = elem.name();
                let m = usize::from(matched);
                let double = script.double();
                let script = if elem == RawElem::Script {
                    script.step(c)
                } else {
                    script
                };
                if m == name.len() + 2 && !double {
                    if c == b'>' {
                        return Self::Text;
                    }
                    if is_space(c) || c == b'/' {
                        return Self::EndTag;
                    }
                }
                let next = match m {
                    0 => (c == b'<').then_some(1),
                    1 => (c == b'/').then_some(2),
                    _ if m - 2 < name.len() => {
                        (c.to_ascii_lowercase() == name[m - 2]).then_some(matched + 1)
                    }
                    _ => None,
                };
                let matched = match next {
                    Some(n) => n,
                    None if c == b'<' => 1,
                    None => 0,
                };
                Self::Raw {
                    elem,
                    matched,
                    sub: sub.step(c, false),
                    script,
                }
            }
        }
    }

    /// What a `{{ }}` here writes.
    pub(crate) fn slot(&self) -> Slot {
        match *self {
            Self::Text => Slot::Text,
            Self::Value { kind, sub, .. } => match (kind, sub) {
                (AttrKind::Html, _) => {
                    Slot::Refused("output inside `srcdoc` is refused: it is parsed as HTML again")
                }
                (AttrKind::Animation, _) => Slot::Refused(
                    "output in an SVG animation value is refused: it can set any attribute, an `href` too",
                ),
                (_, Sub::Url(url)) => Slot::Url(url.part),
                (_, Sub::Js(js)) => Sub::js_slot(js, true),
                (_, Sub::Css(css)) => Sub::css_slot(css, true),
                (_, Sub::JsAmp(_) | Sub::CssAmp(_)) => {
                    Slot::Refused("output right after `&` in an event-handler or style attribute")
                }
                (_, Sub::None) => Slot::Attr,
            },
            Self::Raw { matched, .. } if matched >= 2 => {
                Slot::Refused("output inside what looks like an end tag")
            }
            Self::Raw { script, .. } if script.double() => Slot::Refused(
                "output in a script the HTML parser reads as double-escaped (after `<!--` and `<script`), where `</script>` does not end it",
            ),
            // A value starting with `/` would form an end tag; in a script
            // the value is a literal, which cannot start with `/`.
            Self::Raw { elem, matched, .. } if matched == 1 && elem != RawElem::Script => {
                Slot::Refused("output right after `<` in a raw-text element")
            }
            Self::Raw { sub, .. } => match sub {
                Sub::Js(js) => Sub::js_slot(js, false),
                Sub::Css(css) => Sub::css_slot(css, false),
                _ => Slot::Attr,
            },
            Self::Unquoted { .. } | Self::BeforeValue { .. } => Slot::Refused(
                "output in an unquoted attribute value; put the value in double quotes",
            ),
            Self::Lt | Self::TagName { .. } | Self::EndTag => {
                Slot::Refused("output inside a tag name")
            }
            Self::Tag { .. } | Self::AttrName { .. } | Self::AfterAttrName { .. } => Slot::Refused(
                "output where an attribute name goes; write the attribute itself and output its value",
            ),
            Self::Comment { .. } | Self::Bang { .. } => {
                Slot::Refused("output inside an HTML comment")
            }
            Self::Decl => Slot::Refused("output inside a `<!...>` declaration"),
        }
    }

    fn take_error(&mut self) -> Option<&'static str> {
        if let Self::Value {
            sub: sub @ Sub::Css(Css::Broken),
            ..
        }
        | Self::Raw {
            sub: sub @ Sub::Css(Css::Broken),
            ..
        } = self
        {
            *sub = Sub::Css(Css::Code);
            return Some(
                "`*` right after an output in CSS could open a comment with the value; put a space before it",
            );
        }
        if let Self::Value {
            sub: Sub::Url(url), ..
        } = self
        {
            if url.broken {
                *url = Url {
                    part: UrlPart::Query,
                    open: false,
                    valued: false,
                    broken: false,
                };
                return Some(
                    "a URL scheme could be completed by an output here: a `:` or `&` follows an output before the URL's first `/`, `?`, or `#`; write the scheme in the template (`https://{{ host }}`), or start the output with `/`",
                );
            }
        }
        None
    }

    /// The state after a `{{ }}` wrote something at this place.
    pub(crate) fn after_output(self) -> Self {
        match self {
            Self::Value {
                tag,
                quote,
                kind,
                sub,
            } => Self::Value {
                tag,
                quote,
                kind,
                sub: sub.after_output(),
            },
            Self::Raw {
                elem,
                matched,
                sub,
                script,
            } => Self::Raw {
                elem,
                matched,
                sub: sub.after_output(),
                script,
            },
            other => other,
        }
    }

    /// The state where two branches meet, or `None` when they end in places
    /// that cannot be treated as one.
    pub(crate) fn join(self, other: Self) -> Option<Self> {
        if self == other {
            return Some(self);
        }
        match (self, other) {
            (
                Self::Value {
                    tag,
                    quote,
                    kind,
                    sub: a,
                },
                Self::Value {
                    tag: t2,
                    quote: q2,
                    kind: k2,
                    sub: b,
                },
            ) if tag == t2 && quote == q2 && kind == k2 => a.join(b).map(|sub| Self::Value {
                tag,
                quote,
                kind,
                sub,
            }),
            (
                Self::Raw {
                    elem,
                    matched,
                    sub: a,
                    script,
                },
                Self::Raw {
                    elem: e2,
                    matched: m2,
                    sub: b,
                    script: s2,
                },
            ) if elem == e2 && matched == m2 => {
                let script = script.join(s2)?;
                a.join(b).map(|sub| Self::Raw {
                    elem,
                    matched,
                    sub,
                    script,
                })
            }
            // Between attributes: a branch that added a bare attribute
            // (`{% if x %}hidden{% endif %}`) ends in its name. Joining to the
            // between-attributes state is safe: what follows is read as a new
            // attribute, and output there is refused either way.
            (a, b) => match (a.tag_of(), b.tag_of()) {
                (Some(x), Some(y)) if x == y => Some(Self::Tag { tag: x }),
                _ => None,
            },
        }
    }

    fn tag_of(self) -> Option<Name> {
        match self {
            Self::Tag { tag } | Self::AttrName { tag, .. } | Self::AfterAttrName { tag, .. } => {
                Some(tag)
            }
            _ => None,
        }
    }

    /// A short description for error messages.
    pub(crate) fn describe(&self) -> &'static str {
        match self {
            Self::Text => "HTML text",
            Self::Lt | Self::TagName { .. } | Self::EndTag => "a tag name",
            Self::Tag { .. } | Self::AttrName { .. } | Self::AfterAttrName { .. } => "a tag",
            Self::BeforeValue { .. } | Self::Unquoted { .. } => "an unquoted attribute value",
            Self::Value {
                sub: Sub::Url(_), ..
            } => "a URL attribute value",
            Self::Value {
                sub: Sub::Js(js) | Sub::JsAmp(js),
                ..
            } => js.describe(),
            Self::Value {
                sub: Sub::Css(css) | Sub::CssAmp(css),
                ..
            } => css.describe(),
            Self::Value { .. } => "an attribute value",
            Self::Bang { .. } | Self::Decl => "a declaration",
            Self::Comment { .. } => "an HTML comment",
            Self::Raw {
                elem: RawElem::Script,
                sub: Sub::Js(js),
                ..
            } => js.describe(),
            Self::Raw {
                elem: RawElem::Script,
                ..
            } => "a script element",
            Self::Raw {
                elem: RawElem::Style,
                ..
            } => "a style element",
            Self::Raw {
                elem: RawElem::Title | RawElem::Textarea,
                ..
            } => "a title or textarea element",
            Self::Raw { .. } => "a raw-text element",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(html: &str) -> Slot {
        let mut c = Ctx::Text;
        let _ = c.feed(html.as_bytes());
        c.slot()
    }

    const JS: Slot = Slot::Js {
        string: false,
        attr: false,
    };
    const JS_STR: Slot = Slot::Js {
        string: true,
        attr: false,
    };

    #[test]
    fn slots() {
        assert_eq!(at("<p>"), Slot::Text);
        assert_eq!(at("<p title=\""), Slot::Attr);
        assert_eq!(at("<p title='"), Slot::Attr);
        assert_eq!(at("<p title=\"a\">"), Slot::Text);
        assert_eq!(at("<a href=\""), Slot::Url(UrlPart::Start));
        assert_eq!(at("<a HREF=\"/x/"), Slot::Url(UrlPart::Path));
        assert_eq!(at("<a href=\"/x?q="), Slot::Url(UrlPart::Query));
        assert_eq!(at("<img data-src-url=\""), Slot::Url(UrlPart::Start));
        assert!(matches!(at("<a href="), Slot::Refused(_)));
        assert!(matches!(at("<a href=x"), Slot::Refused(_)));
        assert!(matches!(at("<a "), Slot::Refused(_)));
        assert!(matches!(at("<"), Slot::Refused(_)));
        assert!(matches!(at("<!-- "), Slot::Refused(_)));
        assert_eq!(at("<!-- x -->"), Slot::Text);
        assert_eq!(at("<!-- a -- > b --->"), Slot::Text);
        assert!(matches!(at("<iframe srcdoc=\""), Slot::Refused(_)));
        assert_eq!(at("<script>x</script>"), Slot::Text);
        assert_eq!(at("<SCRIPT>x</Script >"), Slot::Text);
        assert_eq!(at("<script>var s = '</script>"), Slot::Text);
        assert_eq!(at("<title>"), Slot::Attr);
        assert_eq!(at("<textarea><p></textarea>"), Slot::Text);
        assert_eq!(at("a < b"), Slot::Text);
        assert_eq!(at("<!DOCTYPE html>"), Slot::Text);
        assert_eq!(at("<br/>"), Slot::Text);
        assert_eq!(at("<input disabled>"), Slot::Text);
        assert_eq!(at("</div>"), Slot::Text);
    }

    #[test]
    fn script_slots() {
        assert_eq!(at("<script>var a = "), JS);
        assert_eq!(at("<script>var a = '"), JS_STR);
        assert_eq!(at("<script>var a = `x"), JS_STR);
        assert!(matches!(at("<script>// "), Slot::Refused(_)));
        assert!(matches!(at("<script>/* "), Slot::Refused(_)));
        assert!(matches!(at("<script>var r = /a"), Slot::Refused(_)));
        assert!(matches!(at("<script>var a = '</scrip"), Slot::Refused(_)));
        assert_eq!(
            at("<p onclick=\"go("),
            Slot::Js {
                string: false,
                attr: true
            }
        );
        assert_eq!(
            at("<p onclick='go(\""),
            Slot::Js {
                string: true,
                attr: true
            }
        );
        assert_eq!(
            at("<p onclick=\"a && go("),
            Slot::Js {
                string: false,
                attr: true
            }
        );
        assert!(matches!(at("<p onclick=\"go(&quot;"), Slot::Refused(_)));
        assert_eq!(
            at("<style>a { color: "),
            Slot::Css {
                string: false,
                attr: false
            }
        );
        assert_eq!(
            at("<style>a { font: '"),
            Slot::Css {
                string: true,
                attr: false
            }
        );
        assert!(matches!(at("<style>/* "), Slot::Refused(_)));
        assert_eq!(
            at("<p style=\"color: "),
            Slot::Css {
                string: false,
                attr: true
            }
        );
        assert!(matches!(at("<p style=\"x: &quot;"), Slot::Refused(_)));
    }
}
