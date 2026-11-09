//! Property tests: escaping can never be escaped from, and no input makes the
//! lexer, parser, or compiler panic.

use luminess::{Engine, escape};
use proptest::prelude::*;

struct Page {
    title: String,
}
luminess::view!(Page as "page" { title: text });

fn unescape_html(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

fn percent_decode(s: &str) -> Vec<u8> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let hex = |j: usize| b.get(j).is_some_and(u8::is_ascii_hexdigit);
        if b[i] == b'%' && hex(i + 1) && hex(i + 2) {
            let h = u8::from_str_radix(&s[i + 1..i + 3], 16).unwrap();
            out.push(h);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    out
}

fn render(src: &str, title: &str) -> String {
    let engine = Engine::<Page>::builder()
        .template("t", src)
        .build()
        .unwrap();
    engine
        .render_to_string(
            "t",
            &Page {
                title: title.to_owned(),
            },
        )
        .unwrap()
}

/// Text that leans on the characters escaping cares about.
fn nasty() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop_oneof![
            Just("<".to_owned()),
            Just(">".to_owned()),
            Just("&".to_owned()),
            Just("\"".to_owned()),
            Just("'".to_owned()),
            Just("javascript:".to_owned()),
            Just("</script>".to_owned()),
            Just("${".to_owned()),
            Just("\\".to_owned()),
            Just("\u{2028}".to_owned()),
            Just("`".to_owned()),
            Just(";}".to_owned()),
            Just("{{".to_owned()),
            Just("%".to_owned()),
            Just(" ".to_owned()),
            Just("\n".to_owned()),
            any::<char>().prop_map(String::from),
            "[a-z]{1,4}",
        ],
        0..24,
    )
    .prop_map(|v| v.concat())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2000))]

    #[test]
    fn html_round_trips_and_has_no_specials(s in nasty()) {
        let mut out = Vec::new();
        escape::html(&s, &mut out);
        let out = String::from_utf8(out).unwrap();
        prop_assert!(!out.contains(['<', '>', '"', '\'']));
        prop_assert_eq!(unescape_html(&out), s);
    }

    #[test]
    fn text_cannot_open_a_tag(s in nasty()) {
        let out = render("<p>{{ page.title }}</p>", &s);
        let inner = &out[3..out.len() - 4];
        prop_assert!(!inner.contains('<'));
        prop_assert_eq!(unescape_html(inner), s);
    }

    #[test]
    fn attribute_values_cannot_leave_their_quotes(s in nasty()) {
        for (src, q) in [("<a title=\"{{ page.title }}\">", '"'), ("<a title='{{ page.title }}'>", '\'')] {
            let out = render(src, &s);
            let inner = &out[10..out.len() - 2];
            prop_assert!(!inner.contains(q));
            prop_assert_eq!(unescape_html(inner), s.clone());
        }
    }

    #[test]
    fn url_components_round_trip(s in nasty()) {
        let mut out = Vec::new();
        escape::url_component(&s, &mut out);
        let out = String::from_utf8(out).unwrap();
        prop_assert!(out.bytes().all(|c| c.is_ascii_alphanumeric() || b"-._~%".contains(&c)));
        prop_assert_eq!(percent_decode(&out), s.as_bytes());
    }

    #[test]
    fn urls_stay_in_their_attribute(s in nasty()) {
        let out = render("<a href=\"{{ page.title }}\">", &s);
        let inner = &out[9..out.len() - 2];
        prop_assert!(!inner.contains(['"', '\'', '<', '>', ' ', '\n']));
        if !escape::url_allowed(&s) {
            prop_assert_eq!(inner, escape::BLOCKED_URL);
        } else {
            // A URL is normalized, not encoded: escapes already in it stay,
            // so the decoded forms agree.
            prop_assert_eq!(percent_decode(&unescape_html(inner)), percent_decode(&s));
        }
    }

    #[test]
    fn allowed_urls_never_start_with_a_script_scheme(s in nasty()) {
        if escape::url_allowed(&s) {
            let lowered: String = s
                .chars()
                .filter(|c| !matches!(c, '\t' | '\n' | '\r') && *c > ' ')
                .collect::<String>()
                .to_ascii_lowercase();
            let scheme_end = lowered.find([':', '/', '?', '#']);
            if let Some(i) = scheme_end {
                if lowered.as_bytes()[i] == b':' {
                    let scheme = &lowered[..i];
                    prop_assert!(matches!(scheme, "http" | "https" | "mailto" | "tel"), "{scheme}");
                }
            }
        }
    }

    #[test]
    fn compiling_anything_never_panics(src in "[{}%#a-z .|\"'<>=!()\\-/]{0,80}") {
        let _ = Engine::<Page>::builder().template("t", src).compile();
    }

    #[test]
    fn compiling_tag_soup_never_panics(parts in prop::collection::vec(prop_oneof![
        Just("{{ page.title }}"), Just("{% if page.title %}"), Just("{% else %}"), Just("{% endif %}"),
        Just("{% for x in page.title %}"), Just("{% endfor %}"), Just("{% try %}"), Just("{% fallback %}"),
        Just("{% endtry %}"), Just("<a href=\""), Just("\">"), Just("<!--"), Just("-->"), Just("<script>"),
        Just("</script>"), Just("{% raw %}"), Just("{% endraw %}"), Just("{#"), Just("#}"), Just("'"),
        Just("{{-"), Just("-}}"), Just("<p title='"), Just("{% block a %}"), Just("{% endblock %}"),
        Just("{{ super() }}"), Just("{% set x = page.title %}"), Just("{{ x }}"), Just("{{ page.title(1) }}"),
        Just("{{ page.title | truncate(2) | replace(\"a\", \"b\") }}"), Just(r#"{% extends "t" %}"#), Just(r#"{% include "t" with page %}"#), Just("{% include \"t\" %}"),
    ], 0..30)) {
        let src: String = parts.concat();
        if let Ok(t) = Engine::<Page>::builder().template("t", src).compile() {
            let _ = t.render_to_string("t", &Page { title: "<x>".into() });
        }
    }
}

fn js_decode(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next().unwrap() {
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            't' => out.push('\t'),
            'u' => {
                let hex: String = it.by_ref().take(4).collect();
                out.push(char::from_u32(u32::from_str_radix(&hex, 16).unwrap()).unwrap());
            }
            other => out.push(other),
        }
    }
    out
}

fn css_decode(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        let mut hex = String::new();
        while let Some(&h) = it.peek() {
            if h == ' ' {
                let _ = it.next();
                break;
            }
            hex.push(h);
            let _ = it.next();
        }
        out.push(char::from_u32(u32::from_str_radix(&hex, 16).unwrap()).unwrap());
    }
    out
}

struct Script {
    v: String,
}
luminess::view!(Script as "s" { v: text });

fn render_script(src: &str, v: &str) -> String {
    let engine = Engine::<Script>::builder()
        .template("t", src)
        .build()
        .unwrap();
    engine
        .render_to_string("t", &Script { v: v.to_owned() })
        .unwrap()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2000))]

    #[test]
    fn js_strings_round_trip_and_cannot_close_anything(s in nasty()) {
        let mut out = Vec::new();
        escape::js_string(&s, &mut out);
        let out = String::from_utf8(out).unwrap();
        let closes = out.contains(['"', '\'', '`', '<', '>', '&', '\n', '\r', '\u{2028}', '\u{2029}']);
        prop_assert!(!closes, "{out}");
        let opens = out.contains("${");
        prop_assert!(!opens, "{out}");
        prop_assert_eq!(js_decode(&out), s);
    }

    #[test]
    fn script_values_stay_literals(s in nasty()) {
        // In code: one string literal, nothing after it but the template's own `;`.
        let out = render_script("<script>var a = {{ s.v }};</script>", &s);
        let inner = &out["<script>var a = ".len()..out.len() - ";</script>".len()];
        prop_assert!(inner.starts_with('"') && inner.ends_with('"') && inner.len() >= 2);
        prop_assert!(!inner[1..inner.len() - 1].contains('"'));
        prop_assert_eq!(js_decode(&inner[1..inner.len() - 1]), s.clone());
        // In an attribute: the same, with the quotes as entities.
        let out = render_script("<b onclick=\"f({{ s.v }})\">", &s);
        let inner = &out["<b onclick=\"f(".len()..out.len() - ")\">".len()];
        prop_assert!(!inner.contains('"'));
        prop_assert!(inner.starts_with("&quot;") && inner.ends_with("&quot;"));
    }

    #[test]
    fn css_strings_round_trip(s in nasty()) {
        let mut out = Vec::new();
        escape::css(&s, &mut out);
        let out = String::from_utf8(out).unwrap();
        prop_assert!(out.chars().all(|c| c.is_ascii_alphanumeric() || c == '\\' || c == ' '));
        prop_assert_eq!(css_decode(&out), s);
    }

    #[test]
    fn css_values_are_checked(s in nasty()) {
        let out = render_script("<style>a { color: {{ s.v }}; }</style>", &s);
        let inner = &out["<style>a { color: ".len()..out.len() - "; }</style>".len()];
        if escape::css_value_allowed(&s) {
            prop_assert_eq!(inner, s.as_str());
        } else {
            prop_assert_eq!(inner, escape::BLOCKED_CSS);
        }
        let escapes = inner.contains([';', '}', '<', '"', '\'', '\\', ':']) || inner.contains("/*");
        prop_assert!(!escapes, "{inner}");
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    #[test]
    fn application_filter_output_is_escaped(s in nasty()) {
        let engine = Engine::<Page>::builder()
            .filter("echo", &[luminess::Kind::Text], luminess::Kind::Text, |_, args| match args {
                [luminess::Value::Text(t)] => luminess::Computed::Text((*t).to_owned()),
                _ => luminess::Value::Absent.into(),
            })
            .template("t", "<p>{{ page.title | echo(page.title) }}</p>")
            .build()
            .unwrap();
        let out = engine.render_to_string("t", &Page { title: s.clone() }).unwrap();
        let inner = &out[3..out.len() - 4];
        prop_assert!(!inner.contains('<'));
        prop_assert_eq!(unescape_html(inner), s);
    }

    #[test]
    fn truncate_and_number_keep_their_shape(s in nasty(), n in 0usize..30, x in any::<i64>()) {
        struct V { s: String, x: i64 }
        luminess::view!(V as "v" { s: text, x: int });
        let src = format!("{{{{ v.s | truncate({n}) | raw }}}}|{{{{ v.x | number }}}}");
        let engine = Engine::<V>::builder().template("t", src).build().unwrap();
        let out = engine.render_to_string("t", &V { s: s.clone(), x }).unwrap();
        let (cut, num) = out.rsplit_once('|').unwrap();
        if s.chars().count() <= n {
            prop_assert_eq!(cut, s.as_str());
        } else {
            prop_assert_eq!(cut.chars().count(), n + 3);
            prop_assert!(cut.ends_with("..."));
        }
        prop_assert_eq!(num.replace(',', ""), x.to_string());
    }
}
