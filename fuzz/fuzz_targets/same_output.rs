//! Any bytes as a template: this tree and the base release (see Cargo.toml)
//! agree on whether it compiles, and render it byte for byte the same.
//!
//! Except where 0.9.3 reads a template the way a browser does and 0.9.2 did
//! not (URL attributes; scripts across line breaks and HTML-like comments):
//! there the two may differ, but output that both compile must agree outside
//! URL attribute values. Whatever this tree renders never has a
//! `javascript:` URL in an `href`.

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

/// Text the JavaScript tracker now reads as a browser does: line breaks,
/// HTML-like comments, `++` and `--`.
fn breaks(src: &str) -> bool {
    src.contains('\n')
        || src.contains('\r')
        || src.contains("<!")
        || src.contains("--")
        || src.contains("++")
        || src.contains('\u{2028}')
        || src.contains('\u{2029}')
}

/// Places where 0.9.3 reads a template as a browser does and 0.9.2 did not,
/// apart from URL attribute values: outputs may differ.
fn tracker_changed(src: &str) -> bool {
    let lower = src.to_ascii_lowercase();
    (lower.contains("script") && breaks(src))
        || lower.contains("<!")
        || ["xmp", "iframe", "noembed", "noframes", "noscript", "plaintext"]
            .iter()
            .any(|e| lower.contains(e))
        // Long chains, now refused instead of exhausting the stack.
        || src.matches("or").count() + src.matches("and").count() + src.matches('|').count() > 60
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

/// Compile results may differ.
fn changed_on_purpose(src: &str) -> bool {
    let lower = src.to_ascii_lowercase();
    [
        "href",
        "src",
        "action",
        "cite",
        "poster",
        "background",
        "longdesc",
        "usemap",
        "data",
        "manifest",
        "codebase",
        "icon",
        "profile",
        "ping",
        "archive",
        "classid",
        "url",
        "uri",
    ]
    .iter()
    .any(|n| lower.contains(n))
        || tracker_changed(src)
        || breaks(src)
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

fn without_urls(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut at = 0;
    for (start, end) in url_values(s) {
        out.push_str(&s[at..start]);
        at = end;
    }
    out.push_str(&s[at..]);
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
        .compile();
    let base = luminess_base::Engine::<Page>::builder()
        .template("t", src)
        .template("part", "<b>{{ page.title }}</b>")
        .compile();
    let changed = changed_on_purpose(src);
    // The oracle holds for values; a literal `javascript:` (or one spelled
    // with a character reference) is the template's own.
    let lower = src.to_ascii_lowercase();
    let oracle = !lower.contains('j') && !lower.contains('&');
    match (now, base) {
        (Ok(now), Ok(base)) => {
            let a = now.render_to_string("t", &page).unwrap();
            let b = base.render_to_string("t", &page).unwrap();
            assert!(!oracle || no_script_urls(&a), "{a}");
            if tracker_changed(src) {
                // Read differently around a comment, script, or raw-text
                // element: only the URL oracle applies.
            } else if changed {
                assert_eq!(without_urls(&a), without_urls(&b));
            } else {
                assert_eq!(a, b);
            }
        }
        (Err(now), Err(base)) => {
            if !changed {
                assert_eq!(now.to_string(), base.to_string());
            }
        }
        // Branches that end in the same JavaScript string join now.
        (Ok(now), Err(base)) if changed || base.to_string().contains("different places") => {
            let a = now.render_to_string("t", &page).unwrap();
            assert!(!oracle || no_script_urls(&a), "{a}");
        }
        (Err(_), Ok(_)) if changed => {}
        (now, base) => panic!(
            "compile results differ: now ok={} base ok={}",
            now.is_ok(),
            base.is_ok()
        ),
    }
});
