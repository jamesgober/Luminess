//! Tracks where in an HTML document the template's literal text leaves the
//! output, so each `{{ }}` gets the escaper for that place. This is a small
//! subset of the HTML tokenizer: enough to tell text from tags, attribute
//! values of each kind, comments, and raw-text elements.

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
        url: UrlPart,
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
    Raw { elem: RawElem, matched: u8 },
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
        // A long name is classified by its prefix only: an `on*` handler or a
        // name that mentions a URL is treated as such.
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

fn contains(h: &[u8], n: &[u8]) -> bool {
    h.windows(n.len()).any(|w| w == n)
}

fn is_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\r' | b'\x0c')
}

impl Ctx {
    /// The state after a start tag's `>`.
    fn after_tag(tag: Name) -> Self {
        match raw_elem(&tag) {
            Some(elem) => Self::Raw { elem, matched: 0 },
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
                c if c.is_ascii_alphabetic() => {
                    let mut name = Name::EMPTY;
                    name.push(c);
                    Self::TagName { name }
                }
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
                c => {
                    let mut attr = Name::EMPTY;
                    attr.push(c);
                    Self::AttrName { tag, attr }
                }
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
                c => {
                    let mut attr = Name::EMPTY;
                    attr.push(c);
                    Self::AttrName { tag, attr }
                }
            },
            Self::BeforeValue { tag, attr } => match c {
                b'"' | b'\'' => Self::Value {
                    tag,
                    quote: c,
                    kind: attr_kind(&attr),
                    url: UrlPart::Start,
                },
                b'>' => Self::after_tag(tag),
                c if is_space(c) => Self::BeforeValue { tag, attr },
                _ => Self::Unquoted { tag },
            },
            Self::Value {
                tag,
                quote,
                kind,
                url,
            } => {
                if c == quote {
                    Self::Tag { tag }
                } else if kind == AttrKind::Url {
                    let url = match (url, c) {
                        (_, b'?' | b'#') | (UrlPart::Query, _) => UrlPart::Query,
                        _ => UrlPart::Path,
                    };
                    Self::Value {
                        tag,
                        quote,
                        kind,
                        url,
                    }
                } else {
                    self
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
            Self::Raw { elem, matched } => {
                let name = elem.name();
                let m = usize::from(matched);
                let next = match m {
                    0 => (c == b'<').then_some(1),
                    1 => (c == b'/').then_some(2),
                    _ if m - 2 < name.len() => {
                        (c.to_ascii_lowercase() == name[m - 2]).then_some(matched + 1)
                    }
                    _ => {
                        // `</name` read: the element ends at a space, `/`, or `>`.
                        if c == b'>' {
                            return Self::Text;
                        }
                        if is_space(c) || c == b'/' {
                            return Self::EndTag;
                        }
                        None
                    }
                };
                match next {
                    Some(n) => Self::Raw { elem, matched: n },
                    None if c == b'<' => Self::Raw { elem, matched: 1 },
                    None => Self::Raw { elem, matched: 0 },
                }
            }
        }
    }

    /// What a `{{ }}` here writes.
    pub(crate) fn slot(&self) -> Slot {
        match *self {
            Self::Text => Slot::Text,
            Self::Value { kind, url, .. } => match kind {
                AttrKind::Text => Slot::Attr,
                AttrKind::Url => Slot::Url(url),
                AttrKind::Script => Slot::Refused(
                    "output inside an event-handler attribute (`on...`) is not supported yet",
                ),
                AttrKind::Style => {
                    Slot::Refused("output inside a `style` attribute is not supported yet")
                }
                AttrKind::Html => {
                    Slot::Refused("output inside `srcdoc` is refused: it is parsed as HTML again")
                }
            },
            Self::Raw {
                elem: RawElem::Title | RawElem::Textarea,
                ..
            } => Slot::Attr,
            Self::Raw {
                elem: RawElem::Script,
                ..
            } => Slot::Refused("output inside `<script>` is not supported yet"),
            Self::Raw {
                elem: RawElem::Style,
                ..
            } => Slot::Refused("output inside `<style>` is not supported yet"),
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
                kind: AttrKind::Url,
                url: UrlPart::Start,
            } => Self::Value {
                tag,
                quote,
                kind: AttrKind::Url,
                url: UrlPart::Path,
            },
            other => other,
        }
    }

    /// The state where two branches meet, or `None` when they end in places
    /// that cannot be treated as one. Two places in the same URL value join to
    /// the stricter of the two, so an output after them is never under-escaped.
    pub(crate) fn join(self, other: Self) -> Option<Self> {
        if self == other {
            return Some(self);
        }
        match (self, other) {
            (
                Self::Value {
                    tag,
                    quote,
                    kind: AttrKind::Url,
                    url: a,
                },
                Self::Value {
                    tag: t2,
                    quote: q2,
                    kind: AttrKind::Url,
                    url: b,
                },
            ) if tag == t2 && quote == q2 => {
                let url = match (a, b) {
                    (UrlPart::Query, _) | (_, UrlPart::Query) => UrlPart::Query,
                    _ => UrlPart::Start,
                };
                Some(Self::Value {
                    tag,
                    quote,
                    kind: AttrKind::Url,
                    url,
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
                kind: AttrKind::Url,
                ..
            } => "a URL attribute value",
            Self::Value { .. } => "an attribute value",
            Self::Bang { .. } | Self::Decl => "a declaration",
            Self::Comment { .. } => "an HTML comment",
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
        assert!(matches!(at("<p onclick=\""), Slot::Refused(_)));
        assert!(matches!(at("<p style=\""), Slot::Refused(_)));
        assert!(matches!(at("<iframe srcdoc=\""), Slot::Refused(_)));
        assert!(matches!(at("<script>"), Slot::Refused(_)));
        assert!(matches!(at("<script>var a = '</scrip"), Slot::Refused(_)));
        assert_eq!(at("<script>x</script>"), Slot::Text);
        assert_eq!(at("<SCRIPT>x</Script >"), Slot::Text);
        assert_eq!(at("<title>"), Slot::Attr);
        assert_eq!(at("<textarea><p></textarea>"), Slot::Text);
        assert_eq!(at("a < b"), Slot::Text);
        assert_eq!(at("<!DOCTYPE html>"), Slot::Text);
        assert_eq!(at("<br/>"), Slot::Text);
        assert_eq!(at("<input disabled>"), Slot::Text);
        assert_eq!(at("</div>"), Slot::Text);
    }
}
