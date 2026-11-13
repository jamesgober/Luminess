//! Any bytes as a template: lexing, parsing, and compiling never panic, and
//! whatever compiles renders without panicking.

#![no_main]

use libfuzzer_sys::fuzz_target;
use luminess::Engine;

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

fuzz_target!(|data: &[u8]| {
    let Ok(src) = std::str::from_utf8(data) else {
        return;
    };
    // A second template lets includes resolve.
    let built = Engine::<Page>::builder()
        .template("t", src)
        .template("part", "<b>{{ page.title }}</b>")
        .template(
            "base",
            "<b>{% block a %}x{% endblock %}{% block b %}{% endblock %}</b>",
        )
        .build();
    if let Ok(t) = built {
        let page = Page {
            title: "<x> & \"y\"".into(),
            body: "<p>ok</p>".into(),
            count: -3,
            on: true,
            items: vec![
                Item {
                    name: "javascript:1".into(),
                    tags: vec!["a".into()],
                },
                Item {
                    name: String::new(),
                    tags: Vec::new(),
                },
            ],
        };
        let mut out = Vec::new();
        let _ = t.render_into("t", &page, &mut out);
        assert!(std::str::from_utf8(&out).is_ok());
    }
});
