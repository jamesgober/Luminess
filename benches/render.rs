//! Render time for a typical page. Run with `cargo bench --bench render`.
//!
//! The page: a head with a title and meta description, a navigation include
//! with 6 links, an article with a trusted HTML body of about 2 KiB, a list of
//! 20 related posts (title, URL, date, excerpt, tags), and a footer. About
//! 10 KiB of output and 150 values written.
//!
//! No benchmark framework: each case runs for a fixed number of iterations,
//! seven times, and the fastest run is reported (the least disturbed by the
//! rest of the machine).

use std::{hint::black_box, time::Instant};

use luminess::Engine;

struct Link {
    label: String,
    url: String,
}

struct Post {
    title: String,
    url: String,
    date: String,
    excerpt: String,
    tags: Vec<String>,
}

struct Page {
    site: String,
    title: String,
    description: String,
    body: String,
    nav: Vec<Link>,
    related: Vec<Post>,
    year: i64,
    signed_in: bool,
}

luminess::view!(Link as "link" { label: text, url: text });
luminess::view!(Post as "post" { title: text, url: text, date: text, excerpt: text, tags: list(text) });
luminess::view!(Page as "page" {
    site: text, title: text, description: text, body: html,
    nav: list(object(Link)), related: list(object(Post)), year: int, signed_in: bool,
});

const LAYOUT: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <title>{{ page.title }} | {{ page.site }}</title>
  <meta name="description" content="{{ page.description }}">
  <link rel="stylesheet" href="/assets/site.css">
</head>
<body>
{% include "nav" %}
<main>
  <article>
    <h1>{{ page.title }}</h1>
    {{ page.body }}
  </article>
  <section class="related">
    <h2>Related</h2>
    <ul>
    {%- for post in page.related %}
      <li class="{% if loop.first %}first{% endif %}">
        <a href="{{ post.url }}">{{ post.title }}</a>
        <time>{{ post.date }}</time>
        <p>{{ post.excerpt }}</p>
        {%- if post.tags %}
        <ul class="tags">{% for t in post.tags %}<li><a href="/tags/{{ t }}">{{ t }}</a></li>{% endfor %}</ul>
        {%- endif %}
      </li>
    {%- endfor %}
    </ul>
  </section>
</main>
<footer>&copy; {{ page.year }} {{ page.site }}{% if page.signed_in %} | <a href="/logout">Sign out</a>{% endif %}</footer>
</body>
</html>
"#;

const NAV: &str = r#"<nav>
  <ul>
  {%- for l in page.nav %}
    <li><a href="{{ l.url }}">{{ l.label }}</a></li>
  {%- endfor %}
  </ul>
</nav>"#;

fn page() -> Page {
    let body = "<p>Lorem ipsum dolor sit amet, <em>consectetur</em> adipiscing elit. Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.</p>\n".repeat(16);
    Page {
        site: "Example & Co".into(),
        title: "How we render pages in microseconds".into(),
        description: "A look at compiled templates, typed views, and \"contextual\" escaping.".into(),
        body,
        nav: ["Home", "Blog", "Docs", "Pricing", "About", "Contact"]
            .iter()
            .map(|l| Link { label: (*l).into(), url: format!("/{}", l.to_lowercase()) })
            .collect(),
        related: (0..20)
            .map(|i| Post {
                title: format!("Post number {i}: notes on <templates> & speed"),
                url: format!("/blog/2026/11/post-{i}"),
                date: "2026-11-08".into(),
                excerpt: "A short excerpt of the post that runs for a sentence or two, with an apostrophe's worth of escaping.".into(),
                tags: vec!["rust".into(), "web".into()],
            })
            .collect(),
        year: 2026,
        signed_in: true,
    }
}

fn time<F: FnMut()>(name: &str, iters: u32, mut f: F) -> f64 {
    for _ in 0..iters / 10 {
        f();
    }
    let mut best = f64::MAX;
    for _ in 0..7 {
        let t = Instant::now();
        for _ in 0..iters {
            f();
        }
        best = best.min(t.elapsed().as_secs_f64() / f64::from(iters));
    }
    println!("{name:<40} {:>10.2} µs", best * 1e6);
    best
}

fn main() {
    let engine = Engine::<Page>::builder()
        .template("page", LAYOUT)
        .template("nav", NAV)
        .build()
        .expect("compiles");
    let p = page();
    let mut out = Vec::with_capacity(16 * 1024);
    out.clear();
    let _ = engine.render("page", &p, &mut out).expect("renders");
    let bytes = out.len();
    println!("typical page: {bytes} bytes of output");

    time("render typical page (Engine)", 20_000, || {
        out.clear();
        let _ = black_box(engine.render("page", black_box(&p), &mut out));
    });
    let templates = engine.templates();
    time("render typical page (Templates snapshot)", 20_000, || {
        out.clear();
        let _ = black_box(templates.render("page", black_box(&p), &mut out));
    });

    let small = Engine::<Page>::builder()
        .template("t", "<h1>{{ page.title }}</h1>")
        .build()
        .expect("compiles");
    time("render one value", 1_000_000, || {
        out.clear();
        let _ = black_box(small.render("t", black_box(&p), &mut out));
    });

    time("compile the page and its include", 2_000, || {
        let e = Engine::<Page>::builder()
            .template("page", LAYOUT)
            .template("nav", NAV)
            .build();
        let _ = black_box(e);
    });
}
