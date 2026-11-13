//! Renders a small page to standard output: `cargo run --example page`.

use luminess::Engine;

struct Post {
    title: String,
    url: String,
}

struct Page {
    title: String,
    intro: String,
    posts: Vec<Post>,
}

luminess::view!(Post as "post" { title: text, url: text });
luminess::view!(Page as "page" { title: text, intro: html, posts: list(object(Post)) });

const LAYOUT: &str = r#"<!DOCTYPE html>
<title>{{ page.title }}</title>
<h1>{{ page.title }}</h1>
{{ page.intro }}
{% include "list" %}
"#;

const LIST: &str = r#"<ul>
{%- for post in page.posts %}
  <li><a href="{{ post.url }}">{{ loop.index }}. {{ post.title }}</a></li>
{%- else %}
  <li>Nothing yet.</li>
{%- endfor %}
</ul>"#;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let engine = Engine::<Page>::builder()
        .template("page", LAYOUT)
        .template("list", LIST)
        .build()?;
    let page = Page {
        title: "Notes & drafts".into(),
        intro: "<p>Trusted markup is written <em>as is</em>.</p>".into(),
        posts: vec![
            Post {
                title: "Compiled templates".into(),
                url: "/posts/compiled".into(),
            },
            Post {
                title: "A \"suspicious\" link".into(),
                url: "javascript:alert(1)".into(),
            },
        ],
    };
    let mut out = Vec::new();
    let rendered = engine.render_into("page", &page, &mut out)?;
    print!("{}", String::from_utf8(out)?);
    eprintln!("{} bytes", rendered.bytes());
    Ok(())
}
