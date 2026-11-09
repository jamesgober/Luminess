//! Error reports, limits, and checking without keeping the result.

use luminess::{Engine, RenderErrorKind};

struct Page {
    title: String,
    items: Vec<String>,
}
luminess::view!(Page as "page" { title: text, items: list(text) });

fn page() -> Page {
    Page {
        title: "T".into(),
        items: vec!["x".repeat(100); 50],
    }
}

#[test]
fn reports_point_at_the_column() {
    let err = Engine::<Page>::builder()
        .template("a", "line one\n  {{ page.nope }} and\t{{ page.titel }}")
        .template("nav", "{% include \"a\" %}")
        .build()
        .unwrap_err();
    let report = err.report();
    assert!(report.contains("error: `page` (page) has no field `nope`\n --> a:2:11\n  |\n2 |   {{ page.nope }} and {{ page.titel }}\n  |           ^\n"), "{report}");
    assert!(report.contains("did you mean `title`"), "{report}");
    assert_eq!(
        err.diagnostics()[0].source_line(),
        "  {{ page.nope }} and\t{{ page.titel }}"
    );
    let html = err.to_html();
    assert!(
        html.starts_with("<pre class=\"luminess-errors\">error: "),
        "{html}"
    );
    assert!(html.contains("{{ page.nope }}"));
    assert!(!html[28..html.len() - 6].contains('<'));
}

#[test]
fn reports_say_where_a_partial_was_included_from() {
    let err = Engine::<Page>::builder()
        .template("a", "<p title=\"{% include \"b\" %}\">")
        .template("b", "{{ page.nope }}")
        .build()
        .unwrap_err();
    assert!(
        err.report().contains(" = included from a:1"),
        "{}",
        err.report()
    );
}

#[test]
fn output_limit() {
    let engine = Engine::<Page>::builder()
        .max_output(1000)
        .template("big", "{% for i in page.items %}{{ i }}{% endfor %}")
        .template("small", "{{ page.title }}")
        .build()
        .unwrap();
    let mut out = b"keep".to_vec();
    let err = engine.render("big", &page(), &mut out).unwrap_err();
    assert_eq!(err.kind(), RenderErrorKind::TooLarge);
    assert_eq!(
        err.to_string(),
        "the output of `big` passed the limit of 1000 bytes"
    );
    assert_eq!(out, b"keep", "what the render wrote is removed");
    assert_eq!(engine.render_to_string("small", &page()).unwrap(), "T");
}

#[test]
fn template_size_limit() {
    let err = Engine::<Page>::builder()
        .max_template_size(10)
        .template("t", "this is longer than ten bytes")
        .build()
        .unwrap_err();
    assert!(err.to_string().contains("past the limit of 10"), "{err}");
}

#[test]
fn check_keeps_the_builder() {
    let b = Engine::<Page>::builder().template("t", "{{ page.title }}");
    assert!(b.check().is_ok());
    let bad = Engine::<Page>::builder().template("t", "{{ page.nope }}");
    assert!(bad.check().is_err());
    // The builder is still usable after a check.
    let engine = b.build().unwrap();
    assert_eq!(engine.render_to_string("t", &page()).unwrap(), "T");
}
