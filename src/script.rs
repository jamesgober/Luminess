//! Tracks where in JavaScript or CSS the template's literal text leaves the
//! output: in code, in a string, in a comment, or somewhere it cannot tell.
//! Used for `<script>` and `<style>` elements and for `on*` and `style`
//! attributes.

/// What a `/` in JavaScript code starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Slash {
    /// A regular expression (after an operator, `(`, a keyword, ...).
    Regex,
    /// A division (after a value).
    Div,
    /// Two branches disagree; a `/` here loses track.
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum JsMode {
    Code,
    /// After `/` in code: a comment, a regular expression, or a division.
    Slash,
    Str(u8),
    StrEsc(u8),
    Tmpl,
    TmplEsc,
    /// After `$` in a template literal.
    TmplDollar,
    Regex,
    RegexEsc,
    RegexClass,
    RegexClassEsc,
    LineComment,
    /// In a line comment, after the first byte of U+2028 or U+2029.
    LineCommentE2,
    /// After its second byte.
    LineCommentE280,
    BlockComment,
    BlockCommentStar,
    /// The tracker cannot follow the script any more.
    Lost,
}

const WORD_CAP: usize = 10;
const TMPL_CAP: usize = 4;

/// The JavaScript tracker's state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Js {
    pub mode: JsMode,
    slash: Slash,
    /// The identifier being read, for keywords after which `/` is a regex.
    word: [u8; WORD_CAP],
    word_len: u8,
    /// One brace counter per `${` of a template literal being read.
    tmpl: [u8; TMPL_CAP],
    tmpl_len: u8,
    /// How much of `<!--` was just read in code: in a browser it starts a
    /// line comment anywhere (ECMAScript Annex B).
    lt: u8,
    /// How much of `-->` was just read at the start of a line, where it
    /// starts a line comment too.
    dash: u8,
    /// Only whitespace and comments since the last line break.
    line: bool,
    /// The last code byte was a lone `+` or `-`: a second one makes `++` or
    /// `--`, after which `/` means what it meant before the first.
    pm: u8,
    pm_slash: Slash,
}

/// Keywords after which `/` starts a regular expression.
const REGEX_KEYWORDS: [&[u8]; 13] = [
    b"return",
    b"typeof",
    b"case",
    b"do",
    b"else",
    b"in",
    b"instanceof",
    b"new",
    b"delete",
    b"void",
    b"throw",
    b"yield",
    b"await",
];

impl Js {
    pub(crate) const START: Self = Self {
        mode: JsMode::Code,
        slash: Slash::Regex,
        word: [0; WORD_CAP],
        word_len: 0,
        tmpl: [0; TMPL_CAP],
        tmpl_len: 0,
        lt: 0,
        dash: 0,
        line: true,
        pm: 0,
        pm_slash: Slash::Regex,
    };

    pub(crate) const LOST: Self = Self {
        mode: JsMode::Lost,
        ..Self::START
    };

    fn code(mut self, slash: Slash) -> Self {
        self.mode = JsMode::Code;
        self.slash = slash;
        self.clear_word();
        self.lt = 0;
        self.dash = 0;
        self.line = false;
        self.clear_pm();
        self
    }

    /// Clears the identifier, bytes too, so equal states compare equal.
    fn clear_word(&mut self) {
        self.word = [0; WORD_CAP];
        self.word_len = 0;
    }

    fn clear_pm(&mut self) {
        self.pm = 0;
        self.pm_slash = Slash::Regex;
    }

    /// Back in code after a line comment ends.
    fn line_end(mut self) -> Self {
        self.mode = JsMode::Code;
        self.lt = 0;
        self.dash = 0;
        self.line = true;
        self.clear_pm();
        self
    }

    fn with(mut self, mode: JsMode) -> Self {
        self.mode = mode;
        self
    }

    pub(crate) fn step(self, c: u8) -> Self {
        match self.mode {
            JsMode::Code => self.code_byte(c),
            JsMode::Slash => match c {
                b'/' => self.with(JsMode::LineComment),
                b'*' => self.with(JsMode::BlockComment),
                _ => match self.slash {
                    Slash::Regex => self.with(JsMode::Regex).step(c),
                    Slash::Div => self.code(Slash::Regex).step(c),
                    Slash::Unknown => Self::LOST,
                },
            },
            JsMode::Str(q) => match c {
                b'\\' => self.with(JsMode::StrEsc(q)),
                c if c == q => self.code(Slash::Div),
                // A string cannot hold a line break: the script is broken here.
                b'\n' | b'\r' => Self::LOST,
                _ => self,
            },
            JsMode::StrEsc(q) => self.with(JsMode::Str(q)),
            JsMode::Tmpl => match c {
                b'\\' => self.with(JsMode::TmplEsc),
                b'`' => self.code(Slash::Div),
                b'$' => self.with(JsMode::TmplDollar),
                _ => self,
            },
            JsMode::TmplEsc => self.with(JsMode::Tmpl),
            JsMode::TmplDollar => match c {
                b'{' => {
                    let mut s = self;
                    if usize::from(s.tmpl_len) == TMPL_CAP {
                        return Self::LOST;
                    }
                    s.tmpl[usize::from(s.tmpl_len)] = 0;
                    s.tmpl_len += 1;
                    s.code(Slash::Regex)
                }
                _ => self.with(JsMode::Tmpl).step(c),
            },
            JsMode::Regex => match c {
                b'\\' => self.with(JsMode::RegexEsc),
                b'[' => self.with(JsMode::RegexClass),
                b'/' => self.code(Slash::Div),
                _ => self,
            },
            JsMode::RegexEsc => self.with(JsMode::Regex),
            JsMode::RegexClass => match c {
                b'\\' => self.with(JsMode::RegexClassEsc),
                b']' => self.with(JsMode::Regex),
                _ => self,
            },
            JsMode::RegexClassEsc => self.with(JsMode::RegexClass),
            JsMode::LineComment => match c {
                b'\n' | b'\r' => self.line_end(),
                0xE2 => self.with(JsMode::LineCommentE2),
                _ => self,
            },
            JsMode::LineCommentE2 => match c {
                0x80 => self.with(JsMode::LineCommentE280),
                _ => self.with(JsMode::LineComment).step(c),
            },
            JsMode::LineCommentE280 => match c {
                // U+2028 and U+2029 end a line too.
                0xA8 | 0xA9 => self.line_end(),
                _ => self.with(JsMode::LineComment).step(c),
            },
            JsMode::BlockComment => match c {
                b'*' => self.with(JsMode::BlockCommentStar),
                b'\n' | b'\r' => {
                    let mut s = self;
                    s.line = true;
                    s
                }
                _ => self,
            },
            JsMode::BlockCommentStar => match c {
                b'/' => self.with(JsMode::Code),
                b'*' => self,
                b'\n' | b'\r' => {
                    let mut s = self.with(JsMode::BlockComment);
                    s.line = true;
                    s
                }
                _ => self.with(JsMode::BlockComment),
            },
            JsMode::Lost => self,
        }
    }

    fn code_byte(self, c: u8) -> Self {
        let (lt, dash, line) = (self.lt, self.dash, self.line);
        // `<!--` anywhere, and `-->` at the start of a line, begin a line
        // comment in a browser's script.
        if (lt == 3 && c == b'-') || (dash == 2 && c == b'>') {
            let mut s = self.with(JsMode::LineComment);
            s.lt = 0;
            s.dash = 0;
            s.clear_pm();
            return s;
        }
        let mut next = self.code_token(c);
        next.lt = match (lt, c) {
            (_, b'<') => 1,
            (1, b'!') => 2,
            (2, b'-') => 3,
            _ => 0,
        };
        next.dash = match (dash, c) {
            (0, b'-') if line => 1,
            (1, b'-') => 2,
            _ => 0,
        };
        next.line = match c {
            b'\n' | b'\r' => true,
            b' ' | b'\t' | b'\x0c' | b'/' => line,
            _ => false,
        };
        next
    }

    fn code_token(mut self, c: u8) -> Self {
        if c.is_ascii_alphanumeric() || c == b'_' || c == b'$' || c >= 0x80 {
            self.clear_pm();
            if usize::from(self.word_len) < WORD_CAP {
                self.word[usize::from(self.word_len)] = c;
                self.word_len += 1;
                let w = &self.word[..usize::from(self.word_len)];
                self.slash = if REGEX_KEYWORDS.contains(&w) {
                    Slash::Regex
                } else {
                    Slash::Div
                };
            } else {
                self.slash = Slash::Div;
            }
            return self;
        }
        self.clear_word();
        match c {
            b' ' | b'\t' | b'\n' | b'\r' | b'\x0c' => {
                self.clear_pm();
                self
            }
            b'+' | b'-' => {
                if self.pm == c {
                    // `++` or `--`: postfix after a value, prefix after an
                    // operator, so `/` keeps the meaning it had before.
                    let slash = self.pm_slash;
                    self.code(slash)
                } else {
                    let before = self.slash;
                    let mut s = self.code(Slash::Regex);
                    s.pm = c;
                    s.pm_slash = before;
                    s
                }
            }
            b'"' | b'\'' => self.with(JsMode::Str(c)),
            b'`' => self.with(JsMode::Tmpl),
            b'/' => self.with(JsMode::Slash),
            b')' | b']' => self.code(Slash::Div),
            b'{' => {
                if self.tmpl_len > 0 {
                    let i = usize::from(self.tmpl_len) - 1;
                    if self.tmpl[i] == u8::MAX {
                        return Self::LOST;
                    }
                    self.tmpl[i] += 1;
                }
                self.code(Slash::Regex)
            }
            b'}' => {
                if self.tmpl_len > 0 {
                    let i = usize::from(self.tmpl_len) - 1;
                    if self.tmpl[i] == 0 {
                        self.tmpl_len -= 1;
                        return self.with(JsMode::Tmpl);
                    }
                    self.tmpl[i] -= 1;
                }
                self.code(Slash::Regex)
            }
            _ => self.code(Slash::Regex),
        }
    }

    /// The state after a value was written in code.
    pub(crate) fn after_value(self) -> Self {
        if self.mode == JsMode::Code {
            self.code(Slash::Div)
        } else {
            self
        }
    }

    /// Where two branches meet.
    pub(crate) fn join(self, other: Self) -> Option<Self> {
        if self == other {
            return Some(self);
        }
        let same_nesting = self.tmpl_len == other.tmpl_len
            && self.tmpl[..usize::from(self.tmpl_len)] == other.tmpl[..usize::from(other.tmpl_len)];
        // Branches part way into `<!--`, `-->`, or `++` would read what
        // follows differently.
        let same_pending = self.lt == other.lt
            && self.dash == other.dash
            && self.pm == other.pm
            && self.pm_slash == other.pm_slash;
        if self.mode == JsMode::Code && other.mode == JsMode::Code && same_nesting && same_pending {
            let slash = if self.slash == other.slash {
                self.slash
            } else {
                Slash::Unknown
            };
            let mut j = self.code(slash);
            // Treating `-->` as a comment start when either branch would is
            // the safe side: output there is refused.
            j.line = self.line || other.line;
            j.lt = self.lt;
            j.dash = self.dash;
            j.pm = self.pm;
            j.pm_slash = self.pm_slash;
            return Some(j);
        }
        None
    }

    pub(crate) fn describe(self) -> &'static str {
        match self.mode {
            JsMode::Code => "JavaScript code",
            JsMode::Str(_) | JsMode::StrEsc(_) => "a JavaScript string",
            JsMode::Tmpl | JsMode::TmplEsc | JsMode::TmplDollar => "a JavaScript template literal",
            JsMode::Regex | JsMode::RegexEsc | JsMode::RegexClass | JsMode::RegexClassEsc => {
                "a JavaScript regular expression"
            }
            JsMode::LineComment
            | JsMode::LineCommentE2
            | JsMode::LineCommentE280
            | JsMode::BlockComment
            | JsMode::BlockCommentStar => "a JavaScript comment",
            JsMode::Slash => "JavaScript code after `/`",
            JsMode::Lost => "JavaScript the compiler cannot follow",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Css {
    Code,
    /// After `/` in code.
    Slash,
    /// After `\` in code.
    Esc,
    Str(u8),
    StrEsc(u8),
    Comment,
    CommentStar,
    Lost,
}

impl Css {
    pub(crate) fn step(self, c: u8) -> Self {
        match self {
            Self::Code => match c {
                b'"' | b'\'' => Self::Str(c),
                b'/' => Self::Slash,
                b'\\' => Self::Esc,
                _ => Self::Code,
            },
            Self::Slash => match c {
                b'*' => Self::Comment,
                _ => Self::Code.step(c),
            },
            Self::Esc => Self::Code,
            Self::Str(q) => match c {
                b'\\' => Self::StrEsc(q),
                c if c == q => Self::Code,
                b'\n' => Self::Code,
                _ => self,
            },
            Self::StrEsc(q) => Self::Str(q),
            Self::Comment => match c {
                b'*' => Self::CommentStar,
                _ => Self::Comment,
            },
            Self::CommentStar => match c {
                b'/' => Self::Code,
                b'*' => Self::CommentStar,
                _ => Self::Comment,
            },
            Self::Lost => Self::Lost,
        }
    }

    pub(crate) fn describe(self) -> &'static str {
        match self {
            Self::Code | Self::Slash | Self::Esc => "CSS",
            Self::Str(_) | Self::StrEsc(_) => "a CSS string",
            Self::Comment | Self::CommentStar => "a CSS comment",
            Self::Lost => "CSS the compiler cannot follow",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn js(src: &str) -> Js {
        src.bytes().fold(Js::START, Js::step)
    }

    fn css(src: &str) -> Css {
        src.bytes().fold(Css::Code, Css::step)
    }

    #[test]
    fn js_modes() {
        assert_eq!(js("var a = ").mode, JsMode::Code);
        assert_eq!(js("var a = '").mode, JsMode::Str(b'\''));
        assert_eq!(js("var a = 'x\\'").mode, JsMode::Str(b'\''));
        assert_eq!(js("var a = \"x\" + ").mode, JsMode::Code);
        assert_eq!(js("// c").mode, JsMode::LineComment);
        assert_eq!(js("// c\nx = ").mode, JsMode::Code);
        assert_eq!(js("/* c */ x").mode, JsMode::Code);
        assert_eq!(js("x = /re'g/; y = '").mode, JsMode::Str(b'\''));
        assert_eq!(js("x = a / b; y = '").mode, JsMode::Str(b'\''));
        assert_eq!(js("x = /[/']/; y = ").mode, JsMode::Code);
        assert_eq!(js("return /'/.test(s) ? ").mode, JsMode::Code);
        assert_eq!(js("x = `a ${b} c").mode, JsMode::Tmpl);
        assert_eq!(js("x = `a ${ {k: 1}.k } c` + ").mode, JsMode::Code);
        assert_eq!(js("x = `a ${ `b ${c}` ").mode, JsMode::Code);
        assert_eq!(js("x = `$").mode, JsMode::TmplDollar);
    }

    #[test]
    fn js_join() {
        let a = js("x = 1");
        let b = js("x = (");
        let j = a.join(b).unwrap();
        assert_eq!(j.mode, JsMode::Code);
        assert_eq!(j.step(b'/').step(b'x').mode, JsMode::Lost);
        assert_eq!(j.step(b'/').step(b'/').mode, JsMode::LineComment);
        assert!(js("'").join(js("x")).is_none());
    }

    #[test]
    fn css_modes() {
        assert_eq!(css("a { color: "), Css::Code);
        assert_eq!(css("a { font-family: '"), Css::Str(b'\''));
        assert_eq!(css("/* x"), Css::Comment);
        assert_eq!(css("/* x */ a { b: "), Css::Code);
        assert_eq!(css("a { content: \"\\\"\" ; b: "), Css::Code);
    }
}
