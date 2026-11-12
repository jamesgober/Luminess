//! Places where the tracker must read a template the way a browser does, and
//! limits that keep compiling a hostile template bounded. Each case renders a
//! value that would run as script if the tracker lost its place.

use std::time::{Duration, Instant};

use luminess::{Engine, ErrorKind, Schema};

struct P {
    v: String,
    w: String,
}
luminess::view!(P as "p" { v: text, w: text });

const JS: &str = "javascript:alert(1)";
const CODE: &str = "alert(document.domain)";

fn engine(tpl: &str) -> Result<Engine<P>, String> {
    Engine::<P>::builder()
        .template("t", tpl)
        .build()
        .map_err(|e| e.to_string())
}

fn render(tpl: &str, v: &str, w: &str) -> Result<String, String> {
    let e = engine(tpl)?;
    e.render(
        "t",
        &P {
            v: v.into(),
            w: w.into(),
        },
    )
    .map_err(|e| e.to_string())
}

fn ok(tpl: &str, v: &str, w: &str) -> String {
    render(tpl, v, w).unwrap_or_else(|e| panic!("{tpl:?} failed: {e}"))
}

fn refused(tpl: &str) -> String {
    match engine(tpl) {
        Ok(_) => panic!("{tpl:?} compiled"),
        Err(e) => e,
    }
}

// ---------------------------------------------------------------------------
// URL attributes
// ---------------------------------------------------------------------------

#[test]
fn url_whitespace_before_the_output_keeps_the_scheme_check() {
    for tpl in [
        "<a href=\" {{ p.v }}\">x</a>",
        "<a href=\"\n    {{ p.v }}\n  \">x</a>",
        "<a href=\"\t{{ p.v }}\">x</a>",
    ] {
        let out = ok(tpl, JS, "");
        assert!(out.contains("#blocked"), "{tpl:?}: {out}");
        assert!(!out.contains("javascript:"), "{tpl:?}: {out}");
    }
}

#[test]
fn url_second_output_cannot_finish_a_scheme() {
    let tpl = r#"<a href="{{ p.w }}{{ p.v }}">x</a>"#;
    for (v, w) in [
        (JS, ""),
        (":alert(1)", "javascript"),
        ("script:alert(1)", "java"),
    ] {
        let out = ok(tpl, v, w);
        assert!(!out.contains("javascript:"), "{v:?} {w:?}: {out}");
    }
    assert_eq!(ok(tpl, JS, ""), r#"<a href="javascript%3Aalert(1)">x</a>"#);
    // A literal before the output does not settle the scheme either.
    let out = ok(r#"<a href="java{{ p.v }}">x</a>"#, "script:alert(1)", "");
    assert!(!out.contains("javascript:"), "{out}");
}

#[test]
fn url_scheme_completed_by_the_template_is_refused() {
    for tpl in [
        r#"<a href="{{ p.w }}:{{ p.v }}">x</a>"#,
        r#"<a href="{{ p.w }}://{{ p.v }}">x</a>"#,
        r#"<a href="{{ p.w }}&#58;{{ p.v }}">x</a>"#,
        r#"<a href="{{ p.w }}:x">x</a>"#,
        r#"<a href="{% if p.v %}?{% endif %}{{ p.w }}:x">x</a>"#,
    ] {
        let e = refused(tpl);
        assert!(e.contains("scheme"), "{tpl:?}: {e}");
    }
}

#[test]
fn url_common_forms_are_unchanged() {
    let page = |v: &str, w: &str| P {
        v: v.into(),
        w: w.into(),
    };
    let e = Engine::<P>::builder()
        .template("abs", r#"<a href="https://{{ p.w }}/x/{{ p.v }}">x</a>"#)
        .template("mail", r#"<a href="mailto:{{ p.v }}">x</a>"#)
        .template("base", r#"<a href="{{ p.w }}/{{ p.v }}">x</a>"#)
        .template("rel", r#"<a href="/x/{{ p.v }}:{{ p.w }}">x</a>"#)
        .template("query", r#"<a href="?q={{ p.v }}">x</a>"#)
        .template(
            "maybe",
            r#"<a href="{% if p.w %}?q={{ p.w }}{% endif %}">x</a>"#,
        )
        .template("one", r#"<a href="{{ p.v }}">x</a>"#)
        .build()
        .unwrap();
    assert_eq!(
        e.render("abs", &page("a b", "example.com")).unwrap(),
        r#"<a href="https://example.com/x/a%20b">x</a>"#
    );
    assert_eq!(
        e.render("mail", &page("me@example.com", "")).unwrap(),
        r#"<a href="mailto:me@example.com">x</a>"#
    );
    assert_eq!(
        e.render("base", &page("y", "https://example.com")).unwrap(),
        r#"<a href="https://example.com/y">x</a>"#
    );
    assert_eq!(
        e.render("rel", &page("a", "b")).unwrap(),
        r#"<a href="/x/a:b">x</a>"#
    );
    assert_eq!(
        e.render("query", &page("a&b", "")).unwrap(),
        r#"<a href="?q=a%26b">x</a>"#
    );
    assert_eq!(
        e.render("maybe", &page("", "1")).unwrap(),
        r#"<a href="?q=1">x</a>"#
    );
    assert_eq!(
        e.render("one", &page("https://example.com/a", "")).unwrap(),
        r#"<a href="https://example.com/a">x</a>"#
    );
    assert_eq!(
        e.render("one", &page(JS, "")).unwrap(),
        r##"<a href="#blocked">x</a>"##
    );
}

#[test]
fn url_after_a_branch_is_still_checked() {
    let out = ok(
        r#"<a href="{% if p.w %}/x{% endif %}{{ p.v }}">x</a>"#,
        JS,
        "",
    );
    assert!(out.contains("#blocked"), "{out}");
}

// ---------------------------------------------------------------------------
// HTML comments and raw-text elements
// ---------------------------------------------------------------------------

#[test]
fn every_comment_closer_ends_a_comment() {
    for open in ["<!-->", "<!--->", "<!-- x --!>", "<!-- x -->"] {
        let tpl = format!("{open}\n<script>\n-->\n{{{{ p.v }}}}\n</script>");
        let out = ok(&tpl, CODE, "");
        assert!(
            out.contains("\"alert(document.domain)\""),
            "{open:?}: {out}"
        );
    }
    // Still inside: output refused.
    let _ = refused("<!-- x --!{{ p.v }} -->");
    let _ = refused("<!-- x -- {{ p.v }} -->");
}

#[test]
fn raw_text_elements_are_raw_text() {
    for el in ["xmp", "noscript", "iframe", "noembed", "noframes"] {
        let tpl = format!("<{el}><p title='</{el}><img src=x onerror=\"'>{{{{ p.v }}}}\"></{el}>");
        let out = ok(&tpl, "';alert(document.domain)//", "");
        assert!(!out.contains("&#39;;alert"), "{el}: {out}");
        assert!(!out.contains("';alert"), "{el}: {out}");
    }
    // The browser is in a script after `</noscript><script>`: the value is
    // escaped for it, or the template is refused for ending inside it.
    if let Ok(out) = render(
        "<noscript><p title=\"</noscript><script>\">{{ p.v }}</p></noscript>",
        "alert(document.domain)//",
        "",
    ) {
        assert!(!out.contains("alert(document.domain)//"), "{out}");
    }
    // Output in raw text is escaped, so it cannot end the element.
    assert_eq!(
        ok("<noscript>{{ p.v }}</noscript>", "</noscript><b>", ""),
        "<noscript>&lt;/noscript&gt;&lt;b&gt;</noscript>"
    );
    // Plain text runs to the end of the document: its end tag ends nothing,
    // so the template ends inside it.
    let e = refused("<plaintext></plaintext><b>{{ p.v }}");
    assert!(e.contains("raw-text element"), "{e}");
}

// ---------------------------------------------------------------------------
// Scripts
// ---------------------------------------------------------------------------

#[test]
fn html_like_comments_in_scripts_are_comments() {
    for tpl in [
        "<script>\n<!-- don't run on old browsers\nvar n = {{ p.v }};\n</script>",
        "<script>\n<!-- `\nvar n = {{ p.v }};\n</script>",
        "<script>x = 1;\n  --> it's a comment\nvar n = {{ p.v }};\n</script>",
    ] {
        let out = ok(tpl, CODE, "");
        assert!(
            out.contains("var n = \"alert(document.domain)\";"),
            "{tpl:?}: {out}"
        );
    }
    // In the middle of a line `-->` is `--` and `>`.
    let out = ok("<script>while (x-->0) go({{ p.v }});</script>", CODE, "");
    assert!(out.contains("go(\"alert(document.domain)\")"), "{out}");
}

#[test]
fn a_string_does_not_run_past_a_line_break() {
    let e = refused("<script>var s = 'a\nvar n = {{ p.v }};</script>");
    assert!(e.contains("cannot follow"), "{e}");
}

#[test]
fn line_separators_end_a_line_comment() {
    let out = ok(
        "<script>// note\u{2028}var s = {{ p.v }};</script>",
        CODE,
        "",
    );
    assert!(out.contains("var s = \"alert(document.domain)\";"), "{out}");
}

#[test]
fn division_after_postfix_increment() {
    let out = ok(
        r#"<script>var a = i++ / 2, b = "/"; go({{ p.v }});</script>"#,
        CODE,
        "",
    );
    assert!(out.contains("go(\"alert(document.domain)\")"), "{out}");
    let out = ok(
        r#"<script>var a = i-- / 2, b = "/"; go({{ p.v }});</script>"#,
        CODE,
        "",
    );
    assert!(out.contains("go(\"alert(document.domain)\")"), "{out}");
    // A prefix `++` is followed by an operand: `/` starts a regex there.
    let _ = refused("<script>x = ++ /a{{ p.v }}/</script>");
}

#[test]
fn branches_ending_in_the_same_string_join() {
    let out = ok(
        r#"<script>{% if p.w %}x = "{% else %}y = "{% endif %}{{ p.v }}";</script>"#,
        "a\"b",
        "",
    );
    assert_eq!(out, r#"<script>y = "a\u0022b";</script>"#);
}

// ---------------------------------------------------------------------------
// Compile-time limits
// ---------------------------------------------------------------------------

fn on_stack<T: Send + 'static>(stack: usize, f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(stack)
        .spawn(f)
        .unwrap()
        .join()
        .unwrap()
}

#[test]
fn long_operator_chains_are_errors_not_crashes() {
    for (n, op) in [
        (5_000usize, " or p.v"),
        (20_000, " and p.v"),
        (5_000, " | upper"),
    ] {
        let tpl = if op.starts_with(" |") {
            format!("{{{{ p.v{} }}}}", op.repeat(n))
        } else {
            format!("{{% if p.v{} %}}x{{% endif %}}", op.repeat(n))
        };
        let e = on_stack(1 << 20, move || {
            Engine::<P>::builder()
                .template("t", tpl)
                .check()
                .map_err(|e| e.to_string())
        });
        let e = e.unwrap_err();
        assert!(e.contains("too long"), "{e}");
    }
    // A chain of reasonable length still compiles.
    let tpl = format!("{{% if p.v{} %}}x{{% endif %}}", " or p.v".repeat(40));
    assert_eq!(ok(&tpl, "1", ""), "x");
}

#[test]
fn includes_cannot_amplify_without_bound() {
    let part = "x".repeat(64 << 10);
    let main = r#"{% include "part" %}"#.repeat(1_000);
    let t = Instant::now();
    let r = Engine::<P>::builder()
        .template("part", part)
        .template("t", main)
        .build();
    assert!(t.elapsed() < Duration::from_secs(5), "{:?}", t.elapsed());
    let e = r.unwrap_err().to_string();
    assert!(e.contains("MiB of text"), "{e}");
}

#[test]
fn many_literal_pieces_compile_in_linear_time() {
    let tpl = "ab{##}".repeat(80_000);
    let t = Instant::now();
    let out = ok(&tpl, "", "");
    assert!(t.elapsed() < Duration::from_secs(2), "{:?}", t.elapsed());
    assert_eq!(out.len(), 160_000);
}

// ---------------------------------------------------------------------------
// Templates with their own schema
// ---------------------------------------------------------------------------

#[test]
fn a_template_with_another_schema_is_not_rendered_with_the_wrong_view() {
    let other = Schema::build("x").text("title").finish();
    let e = Engine::<P>::builder()
        .template_with_schema("card", "<h1>{{ x.title }}</h1>", other)
        .template("t", "ok")
        .build()
        .unwrap();
    let err = e
        .render(
            "card",
            &P {
                v: "secret".into(),
                w: String::new(),
            },
        )
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::NotFound);

    // A schema the view does answer (a prefix of it) renders by name.
    let prefix = Schema::build("p").text("v").finish();
    let e = Engine::<P>::builder()
        .template_with_schema("short", "<b>{{ p.v }}</b>", prefix)
        .build()
        .unwrap();
    assert_eq!(
        e.render(
            "short",
            &P {
                v: "a".into(),
                w: String::new()
            }
        )
        .unwrap(),
        "<b>a</b>"
    );
}

// ---------------------------------------------------------------------------
// Declared kinds, SVG animation, CSS, and scripts the parser double-escapes
// ---------------------------------------------------------------------------

#[test]
fn text_stays_text_whatever_a_filter_answers() {
    use luminess::{Answer, Kind, Value};
    let e = Engine::<P>::builder()
        .filter("shout", &[], Kind::Text, |v, _| match v {
            Value::Text(s) => Answer::Value(Value::Html(s)),
            other => other.into(),
        })
        .template("t", "<p>{{ p.v | shout }}</p>")
        .build()
        .unwrap();
    let out = e
        .render(
            "t",
            &P {
                v: "<img src=x onerror=alert(1)>".into(),
                w: String::new(),
            },
        )
        .unwrap();
    assert_eq!(out, "<p>&lt;img src=x onerror=alert(1)&gt;</p>");
}

#[test]
fn html_items_of_a_list_are_markup() {
    use luminess::Html;
    struct L {
        items: Vec<Html>,
        plain: Vec<String>,
    }
    luminess::view!(L as "l" { items: list(html), plain: list(text) });
    let t = luminess::Template::<L>::new(
        "{% for i in l.items %}{{ i }}{% endfor %}|{% for i in l.plain %}{{ i }}{% endfor %}",
    )
    .unwrap();
    let out = t
        .render(&L {
            items: vec![Html::from("<b>x</b>")],
            plain: vec!["<b>y</b>".into()],
        })
        .unwrap();
    assert_eq!(out, "<b>x</b>|&lt;b&gt;y&lt;/b&gt;");
}

#[test]
fn svg_animation_values_are_refused() {
    for tpl in [
        r#"<svg><a><animate attributeName="href" values="{{ p.v }}"/></a></svg>"#,
        r#"<svg><a><set attributeName="href" to="{{ p.v }}"/></a></svg>"#,
        r#"<svg><animateTransform from="{{ p.v }}"/></svg>"#,
    ] {
        let e = refused(tpl);
        assert!(e.contains("animation"), "{tpl:?}: {e}");
    }
    // Other attributes of those elements are plain attributes.
    assert_eq!(
        ok(r#"<svg><animate dur="{{ p.v }}"/></svg>"#, "1s", ""),
        r#"<svg><animate dur="1s"/></svg>"#
    );
}

#[test]
fn css_values_cannot_join_the_text_around_them() {
    let _ = refused("<style>a { width: <{{ p.v }} }</style>");
    let _ = refused("<title>a<{{ p.v }}</title>");
    let e = refused("<style>a { width: {{ p.v }}* 2 }</style>");
    assert!(e.contains("open a comment"), "{e}");
    let _ = refused("<style>a { width: {{ p.v }}{{ p.w }} }</style>");
    let _ = refused("<p style=\"width: {{ p.v }}*2\">x</p>");
    // With a space, or after a branch, values are fine.
    assert_eq!(
        ok("<style>a { width: {{ p.v }} * 2 }</style>", "1/", ""),
        "<style>a { width: 1/ * 2 }</style>"
    );
    assert_eq!(
        ok(
            "<style>a { color: {% if p.w %}{{ p.v }}{% endif %} }</style>",
            "red",
            "1"
        ),
        "<style>a { color: red }</style>"
    );
    // In a script a value is a literal, so `<` before it is fine.
    assert_eq!(
        ok("<script>if (i<{{ p.v }}) go();</script>", "3", ""),
        "<script>if (i<\"3\") go();</script>"
    );
}

#[test]
fn double_escaped_script_content_is_followed() {
    // `</script>` inside the double-escaped part does not end the element:
    // the value after it is still script.
    let out = ok(
        "<script>
<!--<script>
<!--</script>
{{ p.v }}
</script>",
        CODE,
        "",
    );
    assert!(
        out.contains(
            "
\"alert(document.domain)\"
"
        ),
        "{out}"
    );
    // Output inside the double-escaped part is refused.
    let e = refused("<script><!--<script> {{ p.v }} </script>--></script>");
    assert!(e.contains("double-escaped"), "{e}");
    // Branches in a script join after text holding `<`, unless one of them
    // is part way into `<!--` or `<script`.
    assert_eq!(
        ok(
            "<script>if (a<b) {% if p.w %}go(){% endif %}x({{ p.v }})</script>",
            "1",
            "1"
        ),
        "<script>if (a<b) go()x(\"1\")</script>"
    );
    let e = refused("<script>{% if p.w %}<!{% endif %}--<script> {{ p.v }}</script>");
    assert!(e.contains("part way into `<!--`"), "{e}");
    // `-->` leaves the escaped states; output after it is followed again.
    let out = ok(
        "<script>
<!--<script>
-->
var a = {{ p.v }};
</script>",
        CODE,
        "",
    );
    assert!(out.contains("var a = \"alert(document.domain)\";"), "{out}");
}

#[test]
fn nested_loops_in_a_script_compile_once() {
    // A loop body in a script must end in the state it started in, as far
    // as what follows is concerned, or each nesting level would compile its
    // body twice: 2^20 times here.
    let mut tpl = String::from("<script>var a = 1;");
    for _ in 0..20 {
        tpl.push_str("{% for i in p.v %}a = 1;");
    }
    for _ in 0..20 {
        tpl.push_str("b = 2;{% endfor %}");
    }
    tpl.push_str("</script>");
    let t = Instant::now();
    let e = refused(&tpl);
    assert!(e.contains("needs a list"), "{e}");
    assert!(!e.contains("too large"), "{e}");
    assert!(
        t.elapsed() < Duration::from_millis(500),
        "{:?}",
        t.elapsed()
    );
}
