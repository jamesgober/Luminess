//! Any bytes as a template: this tree and the base release (see Cargo.toml)
//! agree on whether it compiles, and render it byte for byte the same.

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
                name: String::new(),
                tags: Vec::new(),
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
    match (now, base) {
        (Ok(now), Ok(base)) => {
            assert_eq!(
                now.render_to_string("t", &page).unwrap(),
                base.render_to_string("t", &page).unwrap()
            );
        }
        (Err(now), Err(base)) => assert_eq!(now.to_string(), base.to_string()),
        (now, base) => panic!(
            "compile results differ: now ok={} base ok={}",
            now.is_ok(),
            base.is_ok()
        ),
    }
});
