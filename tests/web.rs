//! The web-net integration, driven the way web-net drives an engine.

#![cfg(feature = "web-net")]

use luminess::{
    Engine,
    web::{ErrorView, Theme},
};
use web_net::{Output, Render};

struct Page {
    title: String,
    items: Vec<String>,
}
luminess::view!(Page as "page" { title: text, items: list(text) });

fn page() -> Page {
    Page {
        title: "Tom & Jerry".into(),
        items: vec!["x".repeat(50); 40],
    }
}

fn render(
    r: &impl Render<View<'static> = Page>,
    name: &str,
    limit: usize,
) -> Result<String, String> {
    let id = r.template(name).ok_or("no such template")?;
    let mut buf = Vec::new();
    let mut out = Output::new(&mut buf, limit);
    match r.render(id, &page(), &mut out) {
        Ok(()) => Ok(String::from_utf8(buf).unwrap()),
        Err(e) => Err(e.code().to_owned()),
    }
}

#[test]
fn renders_by_id_and_refuses_unknown_names() {
    let engine = Engine::<Page>::builder()
        .template("home", "<h1>{{ page.title }}</h1>")
        .template("list", "{% for i in page.items %}{{ i }}{% endfor %}")
        .build()
        .unwrap();
    assert_eq!(
        render(&engine, "home", 1 << 20).unwrap(),
        "<h1>Tom &amp; Jerry</h1>"
    );
    assert!(
        engine.template("nope").is_none(),
        "unknown names stop the app at build"
    );
    // Asking again gives the same id.
    assert_eq!(engine.template("home"), engine.template("home"));
    assert_ne!(engine.template("home"), engine.template("list"));
}

#[test]
fn ids_survive_reloads() {
    let engine = Engine::<Page>::builder()
        .template("a", "A {{ page.title }}")
        .template("b", "B")
        .build()
        .unwrap();
    let a = engine.template("a").unwrap();
    let b = engine.template("b").unwrap();
    // A new theme with the templates in another order, and a new one.
    engine
        .reload(
            Engine::<Page>::builder()
                .template("c", "C")
                .template("b", "B2")
                .template("a", "A2"),
        )
        .unwrap();
    let mut buf = Vec::new();
    Render::render(&engine, a, &page(), &mut Output::new(&mut buf, 1 << 20)).unwrap();
    assert_eq!(buf, b"A2");
    buf.clear();
    Render::render(&engine, b, &page(), &mut Output::new(&mut buf, 1 << 20)).unwrap();
    assert_eq!(buf, b"B2");
    // A reload that drops a template: its id answers not_found.
    engine
        .reload(Engine::<Page>::builder().template("b", "B3"))
        .unwrap();
    buf.clear();
    let err = Render::render(&engine, a, &page(), &mut Output::new(&mut buf, 1 << 20)).unwrap_err();
    assert_eq!(err.code(), "luminess.not_found");
    assert!(err.detail().unwrap().contains("`a`"), "{:?}", err.detail());
}

#[test]
fn ids_work_on_every_thread() {
    let engine = Engine::<Page>::builder()
        .template("home", "{{ page.title }}")
        .build()
        .unwrap();
    let id = engine.template("home").unwrap();
    std::thread::scope(|s| {
        for _ in 0..4 {
            let e = engine.clone();
            s.spawn(move || {
                for _ in 0..100 {
                    let mut buf = Vec::new();
                    Render::render(&e, id, &page(), &mut Output::new(&mut buf, 1 << 20)).unwrap();
                    assert_eq!(buf, b"Tom &amp; Jerry");
                }
            });
        }
    });
}

#[test]
fn output_limits_map_to_codes() {
    let engine = Engine::<Page>::builder()
        .max_output(100)
        .template("list", "{% for i in page.items %}{{ i }}{% endfor %}")
        .build()
        .unwrap();
    assert_eq!(
        render(&engine, "list", 1 << 20).unwrap_err(),
        "luminess.too_large"
    );
}

#[test]
fn themes_render_error_pages() {
    let pages = Engine::<Page>::builder()
        .template("home", "{{ page.title }}")
        .build()
        .unwrap();
    let errors = Engine::<ErrorView>::builder()
        .template("error/404", "<h1>Not here</h1><p>{{ error.reference }}</p>")
        .template("error", "<h1>{{ error.status }} {{ error.title }}</h1>")
        .build()
        .unwrap();
    let theme = Theme::new(pages.clone()).with_error_pages(errors);
    assert_eq!(render(&theme, "home", 1 << 20).unwrap(), "Tom &amp; Jerry");

    // A status with its own template.
    let mut buf = Vec::new();
    let p404 = web_net::ErrorPage::new(404, "Not Found", "Nothing here.", "ref-<1>");
    assert!(
        theme
            .error_page(&p404, &mut Output::new(&mut buf, 1 << 20))
            .unwrap()
    );
    assert_eq!(buf, b"<h1>Not here</h1><p>ref-&lt;1&gt;</p>");
    // Any other status falls back to `error`.
    buf.clear();
    let p500 = web_net::ErrorPage::new(500, "Server Error", "Sorry.", "r2");
    assert!(
        theme
            .error_page(&p500, &mut Output::new(&mut buf, 1 << 20))
            .unwrap()
    );
    assert_eq!(buf, b"<h1>500 Server Error</h1>");
    // Without error templates, web-net's built-in page is used.
    buf.clear();
    assert!(
        !Theme::new(pages)
            .error_page(&p500, &mut Output::new(&mut buf, 1 << 20))
            .unwrap()
    );
    assert!(buf.is_empty());
}
