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
}

/// Where in a URL attribute value the output sits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UrlPart {
    /// Nothing written yet: the scheme is checked.
    Start,
    /// After the start, before `?` or `#`.
    Path,
    /// After `?` or `#`: every reserved character is encoded.
    Query,
}

/// What an attribute value or raw-text element holds, as far as read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Sub {
    None,
    Url(UrlPart),
    Js(Js),
    Css(Css),
    /// A `&` in an `on*` value: a character reference would change what the
    /// script means after decoding, so the next byte decides.
    JsAmp(Js),
    /// The same for a `style` value.
    CssAmp(Css),
}

/// An element whose content is not parsed as HTML.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RawElem {
    Script,
    Style,
    Title,
    Textarea,
}

impl RawElem {
    fn name(self) -> &'static [u8] {
        match self {
            Self::Script => b"script",
            Self::Style => b"style",
            Self::Title => b"title",
            Self::Textarea => b"textarea",
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
    /// Inside `<!-- -->`; `dashes` counts the `-` just read.
    Comment { dashes: u8 },
    /// Inside a raw-text element; `matched` counts the bytes of `</name` read.
    Raw {
        elem: RawElem,
        matched: u8,
        sub: Sub,
    },
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
    [
        RawElem::Script,
        RawElem::Style,
        RawElem::Title,
        RawElem::Textarea,
    ]
    .into_iter()
    .find(|e| tag.is(e.name()))
}

fn attr_kind(attr: &Name) -> AttrKind {
    let n = attr.get();
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
        AttrKind::Url => Sub::Url(UrlPart::Start),
        AttrKind::Script => Sub::Js(Js::START),
        AttrKind::Style => Sub::Css(Css::Code),
        AttrKind::Text | AttrKind::Html => Sub::None,
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
            Self::Url(url) => Self::Url(match (url, c) {
                (_, b'?' | b'#') | (UrlPart::Query, _) => UrlPart::Query,
                _ => UrlPart::Path,
            }),
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
            // Two places in one URL join to the stricter, so an output after
            // them is never under-escaped.
            (Self::Url(a), Self::Url(b)) => Some(Self::Url(match (a, b) {
                (UrlPart::Query, _) | (_, UrlPart::Query) => UrlPart::Query,
                _ => UrlPart::Start,
            })),
            (Self::Js(a), Self::Js(b)) => a.join(b).map(Self::Js),
            _ => None,
        }
    }

    fn after_output(self) -> Self {
        match self {
            Self::Url(UrlPart::Start) => Self::Url(UrlPart::Path),
            Self::Js(js) => Self::Js(js.after_value()),
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
            JsMode::LineComment | JsMode::BlockComment | JsMode::BlockCommentStar => {
                Slot::Refused("output inside a JavaScript comment")
            }
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
                    RawElem::Title | RawElem::Textarea => Sub::None,
                },
            },
            None => Self::Text,
        }
    }

    /// Feeds literal bytes.
    pub(crate) fn feed(&mut self, bytes: &[u8]) {
        for &c in bytes {
            *self = self.step(c);
        }
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
                    let kind = attr_kind(&attr);
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
                (1, b'-') => Self::Comment { dashes: 0 },
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
            Self::Comment { dashes } => match c {
                b'-' => Self::Comment {
                    dashes: dashes.saturating_add(1).min(2),
                },
                b'>' if dashes >= 2 => Self::Text,
                _ => Self::Comment { dashes: 0 },
            },
            Self::Raw { elem, matched, sub } => {
                // The element ends at `</name` whatever the script or style
                // inside it is doing: that is how the HTML parser reads it.
                let name = elem.name();
                let m = usize::from(matched);
                if m == name.len() + 2 {
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
                (_, Sub::Url(url)) => Slot::Url(url),
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
            Self::Raw { elem, matched, sub } => Self::Raw {
                elem,
                matched,
                sub: sub.after_output(),
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
                },
                Self::Raw {
                    elem: e2,
                    matched: m2,
                    sub: b,
                },
            ) if elem == e2 && matched == m2 => {
                a.join(b).map(|sub| Self::Raw { elem, matched, sub })
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
            Self::Raw { .. } => "a title or textarea element",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(html: &str) -> Slot {
        let mut c = Ctx::Text;
        c.feed(html.as_bytes());
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
