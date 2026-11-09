//! The typical page from `benches/render.rs`, rendered by luminess, Tera,
//! MiniJinja, and Askama: `cargo run --release` in this folder.
//!
//! Each engine gets the same data and as close to the same template as its
//! syntax allows. The work differs in one way on purpose: luminess escapes
//! each value for its context (URLs are checked, the script values become
//! JavaScript literals), where the others escape everything as HTML text.
//! Tera and MiniJinja also turn the data into their own values on every
//! render (through serde); luminess and Askama read the Rust values directly.

use std::{hint::black_box, time::Instant};

use askama::Template;
use serde::Serialize;

#[derive(Serialize)]
struct Link {
    label: String,
    url: String,
}

#[derive(Serialize)]
struct Post {
    title: String,
    url: String,
    date: String,
    excerpt: String,
    tags: Vec<String>,
}

#[derive(Serialize)]
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

/// The page in Jinja syntax. `BODY` and `NAV` are filled per engine.
const PAGE: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <title>{{ page.title }} | {{ page.site }}</title>
  <meta name="description" content="{{ page.description }}">
  <link rel="stylesheet" href="/assets/site.css">
</head>
<body>
NAV
<main>
  <article>
    <h1>{{ page.title }}</h1>
    BODY
  </article>
  <section class="related">
    <h2>Related</h2>
    <ul>
    {%- for post in page.related %}
      <li class="{% if loop.first %}first{% endif %}">
        <a href="{{ post.url }}">{{ post.title }}</a>
        <time>{{ post.date }}</time>
        <p>{{ post.excerpt }}</p>
        {%- if TAGS %}
        <ul class="tags">{% for t in post.tags %}<li><a href="/tags/{{ t }}">{{ t }}</a></li>{% endfor %}</ul>
        {%- endif %}
      </li>
    {%- endfor %}
    </ul>
  </section>
</main>
<script>window.page = { title: SCRIPT_TITLE, year: {{ page.year }}, signedIn: {{ page.signed_in }} };</script>
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

fn jinja(body: &str, nav: &str, tags: &str, script_title: &str) -> String {
    PAGE.replace("BODY", body)
        .replace("NAV", nav)
        .replace("TAGS", tags)
        .replace("SCRIPT_TITLE", script_title)
}

#[derive(Template)]
#[template(
    ext = "html",
    source = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <title>{{ page.title }} | {{ page.site }}</title>
  <meta name="description" content="{{ page.description }}">
  <link rel="stylesheet" href="/assets/site.css">
</head>
<body>
<nav>
  <ul>
  {%- for l in page.nav %}
    <li><a href="{{ l.url }}">{{ l.label }}</a></li>
  {%- endfor %}
  </ul>
</nav>
<main>
  <article>
    <h1>{{ page.title }}</h1>
    {{ page.body|safe }}
  </article>
  <section class="related">
    <h2>Related</h2>
    <ul>
    {%- for post in page.related %}
      <li class="{% if loop.first %}first{% endif %}">
        <a href="{{ post.url }}">{{ post.title }}</a>
        <time>{{ post.date }}</time>
        <p>{{ post.excerpt }}</p>
        {%- if !post.tags.is_empty() %}
        <ul class="tags">{% for t in post.tags %}<li><a href="/tags/{{ t }}">{{ t }}</a></li>{% endfor %}</ul>
        {%- endif %}
      </li>
    {%- endfor %}
    </ul>
  </section>
</main>
<script>window.page = { title: "{{ page.title }}", year: {{ page.year }}, signedIn: {{ page.signed_in }} };</script>
<footer>&copy; {{ page.year }} {{ page.site }}{% if page.signed_in %} | <a href="/logout">Sign out</a>{% endif %}</footer>
</body>
</html>
"#
)]
struct AskamaPage<'a> {
    page: &'a Page,
}

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

fn time<F: FnMut() -> usize>(name: &str, iters: u32, mut f: F) {
    let bytes = f();
    for _ in 0..iters / 10 {
        let _ = black_box(f());
    }
    let mut best = f64::MAX;
    for _ in 0..7 {
        let t = Instant::now();
        for _ in 0..iters {
            let _ = black_box(f());
        }
        best = best.min(t.elapsed().as_secs_f64() / f64::from(iters));
    }
    println!("{name:<12} {:>9.2} µs   {bytes:>6} bytes", best * 1e6);
}

fn main() {
    let p = page();
    let iters = 10_000;

    let lum = luminess::Engine::<Page>::builder()
        .template(
            "page",
            jinja(
                "{{ page.body }}",
                "{% include \"nav\" %}",
                "post.tags",
                "{{ page.title }}",
            ),
        )
        .template("nav", NAV)
        .build()
        .expect("luminess compiles");
    let mut out = Vec::with_capacity(16 * 1024);
    time("luminess", iters, || {
        out.clear();
        let _ = lum
            .render("page", black_box(&p), &mut out)
            .expect("renders");
        out.len()
    });

    let mut askama_out = String::with_capacity(16 * 1024);
    let ask = AskamaPage { page: &p };
    time("askama", iters, || {
        askama_out.clear();
        ask.render_into(&mut askama_out).expect("renders");
        askama_out.len()
    });

    let mut mj = minijinja::Environment::new();
    mj.add_template_owned(
        "page",
        jinja(
            "{{ page.body | safe }}",
            "{% include \"nav\" %}",
            "post.tags",
            "{{ page.title | tojson }}",
        ),
    )
    .expect("minijinja compiles");
    mj.add_template("nav", NAV).expect("minijinja compiles");
    let mj_page = mj.get_template("page").expect("template");
    time("minijinja", iters, || {
        let s = mj_page
            .render(
                minijinja::context! { page => minijinja::Value::from(minijinja::value::Serde(&p)) },
            )
            .expect("renders");
        s.len()
    });

    let mut tera = tera::Tera::default();
    tera.add_raw_templates(vec![
        (
            "page",
            jinja(
                "{{ page.body | safe }}",
                "{% include \"nav\" %}",
                "post.tags",
                "\"{{ page.title }}\"",
            ),
        ),
        ("nav", NAV.to_owned()),
    ])
    .expect("tera compiles");
    time("tera", iters, || {
        let mut ctx = tera::Context::new();
        ctx.insert("page", &p);
        let s = tera.render("page", &ctx).expect("renders");
        s.len()
    });
}
