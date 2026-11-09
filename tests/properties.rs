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
        Just("{{-"), Just("-}}"), Just("<p title='"), Just("{% include \"t\" %}"),
    ], 0..30)) {
        let src: String = parts.concat();
        if let Ok(t) = Engine::<Page>::builder().template("t", src).compile() {
            let _ = t.render_to_string("t", &Page { title: "<x>".into() });
        }
    }
}
