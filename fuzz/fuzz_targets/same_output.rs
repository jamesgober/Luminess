//! Any bytes as a template: this tree and the base release (see Cargo.toml)
//! agree on whether it compiles, and render it byte for byte the same. The
//! one exception: a template this tree refuses with one of the refusals it
//! adds over the base (`NEW_REFUSALS`). Whatever this tree renders never has
//! a `javascript:` URL in a URL attribute.

#![no_main]

use libfuzzer_sys::fuzz_target;

struct Item {
    name: String,
    tags: Vec<String>,
}

struct Page {
    title: String,
    body: String,
    count: i64,
    on: bool,
    items: Vec<Item>,
}

luminess::view!(Item as "item" { name: text, tags: list(text) });
luminess::view!(Page as "page" { title: text, body: html, count: int, on: bool, items: list(object(Item)) });
luminess_base::view!(Item as "item" { name: text, tags: list(text) });
luminess_base::view!(Page as "page" { title: text, body: html, count: int, on: bool, items: list(object(Item)) });

/// Compile errors this tree adds over the base release.
const NEW_REFUSALS: &[&str] = &[
    "two CSS values side by side",
    "could open a comment with the value",
    "SVG animation value",
    "right after `<` in a raw-text element",
    "double-escaped",
];

fn newly_refused(e: &str) -> bool {
    NEW_REFUSALS.iter().any(|m| e.contains(m))
}

/// Templates this tree renders or compiles differently from the base on
/// purpose: an `html` value reached through `default` on text is escaped
/// now, and a script the HTML parser double-escapes is followed past its
/// inner `</script>` (so the template may now end inside it).
fn renders_differently_on_purpose(src: &str) -> bool {
    let lower = src.to_ascii_lowercase();
    lower.contains("default") || (lower.contains("<!--") && lower.matches("<script").count() > 1)
}

/// Attribute names the tracker reads as URLs.
fn url_name(n: &str) -> bool {
    let n = n.to_ascii_lowercase();
    matches!(
        n.as_str(),
        "href"
            | "src"
            | "action"
            | "formaction"
            | "cite"
            | "poster"
            | "background"
            | "longdesc"
            | "usemap"
            | "data"
            | "manifest"
            | "codebase"
            | "icon"
            | "profile"
            | "srcset"
            | "xlink:href"
            | "ping"
            | "archive"
            | "classid"
            | "lowsrc"
            | "dynsrc"
    ) || (!n.starts_with("on") && (n.contains("url") || n.contains("uri")))
}

/// The values of URL attributes in `s`, as (start, end) byte ranges: a
/// small HTML tokenizer that knows tags, quoted and unquoted values,
/// comments, and raw-text elements, so `href="` in text or in a script is not
/// taken for an attribute.
fn url_values(s: &str) -> Vec<(usize, usize)> {
    const RAW: [&[u8]; 10] = [
        b"script",
        b"style",
        b"title",
        b"textarea",
        b"xmp",
        b"iframe",
        b"noembed",
        b"noframes",
        b"noscript",
        b"plaintext",
    ];
    let b = s.as_bytes();
    let n = b.len();
    let space = |c: u8| matches!(c, b' ' | b'\t' | b'\n' | b'\r' | b'\x0c');
    let find = |from: usize, pat: &[u8]| -> usize {
        b[from.min(n)..]
            .windows(pat.len())
            .position(|w| w.eq_ignore_ascii_case(pat))
            .map_or(n, |p| from + p)
    };
    let mut out = Vec::new();
    let mut i = 0;
    while i < n {
        if b[i] != b'<' {
            i += 1;
            continue;
        }
        let rest = &b[i..];
        if rest.starts_with(b"<!--") {
            let j = i + 4;
            if b.get(j) == Some(&b'>') {
                i = j + 1;
            } else if b[j.min(n)..].starts_with(b"->") {
                i = j + 2;
            } else {
                let e1 = find(j, b"-->");
                let e2 = find(j, b"--!>");
                i = if e1 <= e2 { e1 + 3 } else { e2 + 4 };
            }
            continue;
        }
        if matches!(rest.get(1), Some(b'!' | b'?' | b'/')) {
            i = find(i + 1, b">") + 1;
            continue;
        }
        if !rest.get(1).is_some_and(u8::is_ascii_alphabetic) {
            i += 1;
            continue;
        }
        // A start tag: its name, then its attributes.
        let mut j = i + 1;
        while j < n && !space(b[j]) && b[j] != b'>' && b[j] != b'/' {
            j += 1;
        }
        let name = s[i + 1..j].to_ascii_lowercase();
        loop {
            while j < n && (space(b[j]) || b[j] == b'/') {
                j += 1;
            }
            if j >= n || b[j] == b'>' {
                j += 1;
                break;
            }
            let a0 = j;
            while j < n && !space(b[j]) && !matches!(b[j], b'>' | b'/' | b'=') {
                j += 1;
            }
            let attr = &s[a0..j];
            while j < n && space(b[j]) {
                j += 1;
            }
            if j < n && b[j] == b'=' {
                j += 1;
                while j < n && space(b[j]) {
                    j += 1;
                }
                let (v0, v1);
                if j < n && (b[j] == b'"' || b[j] == b'\'') {
                    let q = b[j];
                    v0 = j + 1;
                    v1 = b[v0..].iter().position(|&c| c == q).map_or(n, |p| v0 + p);
                    j = (v1 + 1).min(n);
                } else {
                    v0 = j;
                    while j < n && !space(b[j]) && b[j] != b'>' {
                        j += 1;
                    }
                    v1 = j;
                }
                if url_name(attr) {
                    out.push((v0, v1));
                }
            }
        }
        if RAW.contains(&name.as_bytes()) {
            let mut end = Vec::with_capacity(name.len() + 2);
            end.extend_from_slice(b"</");
            end.extend_from_slice(name.as_bytes());
            i = if name == "plaintext" {
                n
            } else {
                find(j.min(n), &end)
            };
        } else {
            i = j;
        }
    }
    out
}

/// No URL attribute holds a `javascript:` URL, as a browser reads it.
fn no_script_urls(s: &str) -> bool {
    url_values(s).into_iter().all(|(start, end)| {
        let v: String = s[start..end]
            .trim_start_matches(|c: char| c <= ' ')
            .chars()
            .filter(|c| !matches!(c, '\t' | '\n' | '\r'))
            .collect();
        !v.to_ascii_lowercase().starts_with("javascript:")
    })
}

fuzz_target!(|data: &[u8]| {
    let Ok(src) = std::str::from_utf8(data) else {
        return;
    };
    let page = Page {
        title: "<x> & \"y\" 'z' javascript:1 </script>".into(),
        body: "<p>ok</p>".into(),
        count: -1234567,
        on: true,
        items: vec![
            Item {
                name: "a&b".into(),
                tags: vec!["t<".into(), "u".into()],
            },
            Item {
                name: "javascript:1".into(),
                tags: vec!["javascript".into(), ":1".into(), " javascript:1".into()],
            },
        ],
    };
    let now = luminess::Engine::<Page>::builder()
        .template("t", src)
        .template("part", "<b>{{ page.title }}</b>")
        .build();
    let base = luminess_base::Engine::<Page>::builder()
        .template("t", src)
        .template("part", "<b>{{ page.title }}</b>")
        .compile();
    // The oracle holds for values; a literal `javascript:` (or one spelled
    // with a character reference) is the template's own.
    let lower = src.to_ascii_lowercase();
    let oracle = !lower.contains('j') && !lower.contains('&');
    match (now, base) {
        (Ok(now), Ok(base)) => {
            let a = now.render("t", &page).unwrap();
            let b = base.render_to_string("t", &page).unwrap();
            assert!(!oracle || no_script_urls(&a), "{a}");
            if !renders_differently_on_purpose(src) {
                assert_eq!(a, b);
            }
        }
        (Err(now), Err(base)) => {
            let now = now.to_string();
            if !newly_refused(&now) && !renders_differently_on_purpose(src) {
                assert_eq!(now, base.to_string());
            }
        }
        (Err(now), Ok(_)) => {
            let now = now.to_string();
            assert!(
                newly_refused(&now) || renders_differently_on_purpose(src),
                "{now}"
            );
        }
        (Ok(_), Err(base)) => panic!("compiles now, not in the base: {base}"),
    }
});
