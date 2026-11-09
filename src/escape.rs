//! The escapers the engine picks from, public so a block's own renderer and
//! the application's code can use the same ones.
//!
//! Every function appends to a byte buffer and never fails. The output is
//! valid UTF-8 whenever the input is.

/// Bytes [`html`] replaces.
const fn html_table() -> [u8; 256] {
    let mut t = [0u8; 256];
    t[b'&' as usize] = 1;
    t[b'<' as usize] = 2;
    t[b'>' as usize] = 3;
    t[b'"' as usize] = 4;
    t[b'\'' as usize] = 5;
    t
}
static HTML: [u8; 256] = html_table();
const HTML_REPL: [&[u8]; 6] = [b"", b"&amp;", b"&lt;", b"&gt;", b"&quot;", b"&#39;"];

/// Escapes text for HTML text or a quoted attribute value: `&`, `<`, `>`, `"`,
/// and `'` become entities. Everything else is copied.
///
/// ```
/// let mut out = Vec::new();
/// luminess::escape::html("<a href='x'>Tom & Jerry</a>", &mut out);
/// assert_eq!(out, b"&lt;a href=&#39;x&#39;&gt;Tom &amp; Jerry&lt;/a&gt;");
/// ```
pub fn html(s: &str, out: &mut Vec<u8>) {
    let b = s.as_bytes();
    let mut start = 0;
    for (i, &c) in b.iter().enumerate() {
        let r = HTML[c as usize];
        if r != 0 {
            out.extend_from_slice(&b[start..i]);
            out.extend_from_slice(HTML_REPL[r as usize]);
            start = i + 1;
        }
    }
    out.extend_from_slice(&b[start..]);
}

/// What [`url`] writes in place of a URL whose scheme is not allowed.
pub const BLOCKED_URL: &str = "#blocked";

/// `true` when `s` is safe as the start of a URL attribute: a relative URL, or
/// an absolute one with the `http`, `https`, `mailto`, or `tel` scheme.
///
/// ```
/// use luminess::escape::url_allowed;
/// assert!(url_allowed("/posts/1"));
/// assert!(url_allowed("https://example.com"));
/// assert!(url_allowed("mailto:me@example.com"));
/// assert!(!url_allowed("javascript:alert(1)"));
/// assert!(!url_allowed(" JavaScript:alert(1)"));
/// assert!(!url_allowed("data:text/html,x"));
/// ```
#[must_use]
pub fn url_allowed(s: &str) -> bool {
    // A browser strips leading control characters and spaces and ignores tabs
    // and newlines inside the scheme, so the check looks past them too.
    let mut scheme = [0u8; 8];
    let mut n = 0;
    for &c in s.as_bytes() {
        match c {
            b':' => {
                return matches!(&scheme[..n], b"http" | b"https" | b"mailto" | b"tel");
            }
            b'/' | b'?' | b'#' => return true,
            b'\t' | b'\n' | b'\r' => {}
            0..=0x20 if n == 0 => {}
            c if c.is_ascii_alphanumeric() || c == b'+' || c == b'-' || c == b'.' => {
                if n == scheme.len() {
                    // Longer than every allowed scheme: allowed only if no
                    // colon follows before the path starts.
                    return !has_scheme_colon(s);
                }
                scheme[n] = c.to_ascii_lowercase();
                n += 1;
            }
            // Any other byte cannot be part of a scheme, so the URL is
            // relative unless a colon still follows (`a b:c` is not a scheme,
            // but a browser could read a later colon after stripping).
            _ => return !has_scheme_colon(s),
        }
    }
    true
}

/// `true` when a colon appears before the first `/`, `?`, or `#`.
fn has_scheme_colon(s: &str) -> bool {
    for &c in s.as_bytes() {
        match c {
            b':' => return true,
            b'/' | b'?' | b'#' => return false,
            _ => {}
        }
    }
    false
}

/// Bytes a URL may keep as they are: unreserved, reserved, and `%`.
const fn url_keep_table() -> [bool; 256] {
    let mut t = [false; 256];
    let mut c = 0;
    while c < 256 {
        let b = c as u8;
        t[c] = b.is_ascii_alphanumeric()
            || matches!(
                b,
                b'-' | b'.'
                    | b'_'
                    | b'~'
                    | b':'
                    | b'/'
                    | b'?'
                    | b'#'
                    | b'['
                    | b']'
                    | b'@'
                    | b'!'
                    | b'$'
                    | b'&'
                    | b'('
                    | b')'
                    | b'*'
                    | b'+'
                    | b','
                    | b';'
                    | b'='
                    | b'%'
            );
        c += 1;
    }
    t
}
static URL_KEEP: [bool; 256] = url_keep_table();

const fn component_keep_table() -> [bool; 256] {
    let mut t = [false; 256];
    let mut c = 0;
    while c < 256 {
        let b = c as u8;
        t[c] = b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~');
        c += 1;
    }
    t
}
static COMPONENT_KEEP: [bool; 256] = component_keep_table();

const HEX: &[u8; 16] = b"0123456789ABCDEF";

fn percent(c: u8, out: &mut Vec<u8>) {
    out.extend_from_slice(&[b'%', HEX[usize::from(c >> 4)], HEX[usize::from(c & 15)]]);
}

/// Writes `s` as a URL inside a quoted attribute value: bytes that are not
/// allowed in a URL are percent-encoded, a `%` that does not start a valid
/// escape becomes `%25`, and `&` becomes `&amp;`. The scheme is not checked;
/// see [`url_allowed`].
///
/// ```
/// let mut out = Vec::new();
/// luminess::escape::url("/a b?x=1&y=\"2\"&z=%41%", &mut out);
/// assert_eq!(out, b"/a%20b?x=1&amp;y=%222%22&amp;z=%41%25");
/// ```
pub fn url(s: &str, out: &mut Vec<u8>) {
    let b = s.as_bytes();
    for (i, &c) in b.iter().enumerate() {
        if c == b'&' {
            out.extend_from_slice(b"&amp;");
        } else if c == b'%' {
            let valid = b.get(i + 1).is_some_and(u8::is_ascii_hexdigit)
                && b.get(i + 2).is_some_and(u8::is_ascii_hexdigit);
            if valid {
                out.push(b'%');
            } else {
                out.extend_from_slice(b"%25");
            }
        } else if URL_KEEP[c as usize] {
            out.push(c);
        } else {
            percent(c, out);
        }
    }
}

/// Writes `s` as one URL component (a query value, a path segment): every byte
/// other than letters, digits, `-`, `.`, `_`, and `~` is percent-encoded.
///
/// ```
/// let mut out = Vec::new();
/// luminess::escape::url_component("a&b=c d/é", &mut out);
/// assert_eq!(out, b"a%26b%3Dc%20d%2F%C3%A9");
/// ```
pub fn url_component(s: &str, out: &mut Vec<u8>) {
    for &c in s.as_bytes() {
        if COMPONENT_KEEP[c as usize] {
            out.push(c);
        } else {
            percent(c, out);
        }
    }
}

/// Writes `s` as the content of a JavaScript string literal (single, double,
/// or backtick quoted). Quotes, backslashes, `/`, `<`, `>`, `&`, `$`, `{`,
/// `}`, `=`, line breaks, control characters, and U+2028/U+2029 are escaped,
/// so the content can end neither the string nor the script element around
/// it.
///
/// ```
/// let mut out = Vec::new();
/// luminess::escape::js_string("it's </script>", &mut out);
/// assert_eq!(out, br"it\u0027s \u003C\/script\u003E");
/// ```
pub fn js_string(s: &str, out: &mut Vec<u8>) {
    let b = s.as_bytes();
    let mut start = 0;
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        let short: &[u8] = match c {
            b'\\' => br"\\",
            b'/' => br"\/",
            b'\n' => br"\n",
            b'\r' => br"\r",
            b'\t' => br"\t",
            b'"'
            | b'\''
            | b'`'
            | b'<'
            | b'>'
            | b'&'
            | b'$'
            | b'{'
            | b'}'
            | b'='
            | 0..=0x1f
            | 0x7f => b"",
            // U+2028 and U+2029 end a line in older JavaScript.
            0xe2 if b.get(i + 1) == Some(&0x80) && matches!(b.get(i + 2), Some(0xa8 | 0xa9)) => {
                out.extend_from_slice(&b[start..i]);
                out.extend_from_slice(if b[i + 2] == 0xa8 {
                    br"\u2028"
                } else {
                    br"\u2029"
                });
                i += 3;
                start = i;
                continue;
            }
            _ => {
                i += 1;
                continue;
            }
        };
        out.extend_from_slice(&b[start..i]);
        if short.is_empty() {
            out.extend_from_slice(br"\u00");
            out.extend_from_slice(&[HEX[usize::from(c >> 4)], HEX[usize::from(c & 15)]]);
        } else {
            out.extend_from_slice(short);
        }
        i += 1;
        start = i;
    }
    out.extend_from_slice(&b[start..]);
}

/// Writes `s` as a double-quoted JavaScript string literal, quotes included.
///
/// ```
/// let mut out = Vec::new();
/// luminess::escape::js_quoted("a\"b", &mut out);
/// assert_eq!(out, br#""a\u0022b""#);
/// ```
pub fn js_quoted(s: &str, out: &mut Vec<u8>) {
    out.push(b'"');
    js_string(s, out);
    out.push(b'"');
}

/// Writes `s` for a CSS string: every character other than ASCII letters and
/// digits becomes a hex escape (`\HH `), which CSS reads back as the same
/// character and which cannot end the string, the declaration, or the style
/// element.
///
/// ```
/// let mut out = Vec::new();
/// luminess::escape::css("a'b;}", &mut out);
/// assert_eq!(out, br"a\27 b\3b \7d ");
/// ```
pub fn css(s: &str, out: &mut Vec<u8>) {
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch as u8);
        } else {
            let mut buf = [0u8; 8];
            let mut n = u32::from(ch);
            let mut p = buf.len();
            loop {
                p -= 1;
                buf[p] = b"0123456789abcdef"[(n & 15) as usize];
                n >>= 4;
                if n == 0 {
                    break;
                }
            }
            out.push(b'\\');
            out.extend_from_slice(&buf[p..]);
            out.push(b' ');
        }
    }
}

/// What a CSS value that [`css_value_allowed`] refuses is replaced with.
pub const BLOCKED_CSS: &str = "blocked";

/// `true` when `s` can be written as a CSS value as it is: only ASCII
/// letters, digits, spaces, `#`, `%`, `.`, `,`, `-`, and `_`. That covers
/// colors, lengths, keywords, and font names; anything else (a `(`, a `:`, a
/// quote, a backslash) could start a function or end the declaration.
///
/// ```
/// use luminess::escape::css_value_allowed;
/// assert!(css_value_allowed("#ff0000"));
/// assert!(css_value_allowed("1.5em"));
/// assert!(!css_value_allowed("red; background: url(x)"));
/// assert!(!css_value_allowed("expression(alert(1))"));
/// ```
#[must_use]
pub fn css_value_allowed(s: &str) -> bool {
    s.bytes().all(|c| {
        c.is_ascii_alphanumeric() || matches!(c, b' ' | b'#' | b'%' | b'.' | b',' | b'-' | b'_')
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn js(s: &str) -> String {
        let mut o = Vec::new();
        js_string(s, &mut o);
        String::from_utf8(o).unwrap()
    }

    #[test]
    fn js_escapes() {
        assert_eq!(js("plain é"), "plain é");
        assert_eq!(js("\u{2028}\u{2029}"), r"\u2028\u2029");
        assert_eq!(js("a\u{0}b\u{1f}"), r"a\u0000b\u001F");
        assert_eq!(js("${x}`"), r"\u0024\u007Bx\u007D\u0060");
        assert_eq!(js("\\\n"), r"\\\n");
        assert_eq!(js("\u{2027}"), "\u{2027}");
    }

    #[test]
    fn css_escapes() {
        let mut o = Vec::new();
        css("é\u{1F600}", &mut o);
        assert_eq!(o, br"\e9 \1f600 ");
    }

    fn h(s: &str) -> String {
        let mut o = Vec::new();
        html(s, &mut o);
        String::from_utf8(o).unwrap()
    }

    #[test]
    fn html_escapes_the_five() {
        assert_eq!(h(""), "");
        assert_eq!(h("plain"), "plain");
        assert_eq!(h("&<>\"'"), "&amp;&lt;&gt;&quot;&#39;");
        assert_eq!(h("é<é"), "é&lt;é");
    }

    #[test]
    fn schemes() {
        for ok in [
            "",
            "x",
            "/x",
            "?q",
            "#f",
            "http://a",
            "HTTPS://a",
            "tel:+1",
            "a/b:c",
            "./a:b",
        ] {
            assert!(url_allowed(ok), "{ok}");
        }
        for bad in [
            "javascript:x",
            "JAVASCRIPT:x",
            "\u{1}javascript:x",
            "java\tscript:x",
            "vbscript:x",
            "data:x",
            "file:x",
            "httpsx:x",
            "a b:c",
            "verylongscheme:x",
        ] {
            assert!(!url_allowed(bad), "{bad}");
        }
    }
}
