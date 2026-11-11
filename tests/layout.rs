//! Layouts (`extends`, `block`, `super()`), partials with their own root
//! (`include ... with`), and loading templates from a folder.

use luminess::{Engine, Schema};

struct Author {
    name: String,
}

struct Post {
    title: String,
    author: Author,
}

struct Page {
    title: String,
    posts: Vec<Post>,
}

luminess::view!(Author as "author" { name: text });
luminess::view!(Post as "post" { title: text, author: object(Author) });
luminess::view!(Page as "page" { title: text, posts: list(object(Post)) });

const BASE: &str = r#"<html><head><title>{% block title %}Site{% endblock %}</title></head>
<body>{% block body %}<p>empty</p>{% endblock %}<footer>{% block footer %}(c){% endblock footer %}</footer></body></html>"#;

fn page() -> Page {
    Page {
        title: "News".into(),
        posts: vec![
            Post {
                title: "One".into(),
                author: Author { name: "Ann".into() },
            },
            Post {
                title: "Two & more".into(),
                author: Author { name: "Bo".into() },
            },
        ],
    }
}

fn build(templates: &[(&str, &str)]) -> Result<Engine<Page>, luminess::Error> {
    let mut b = Engine::<Page>::builder();
    for (n, s) in templates {
        b = b.template(*n, *s);
    }
    b.build()
}

fn render(templates: &[(&str, &str)], name: &str) -> String {
    let e = build(templates).unwrap_or_else(|e| panic!("{e}"));
    e.render(name, &page()).unwrap()
}

fn error(templates: &[(&str, &str)]) -> String {
    match build(templates) {
        Ok(_) => panic!("expected a compile error"),
        Err(e) => e.to_string(),
    }
}

#[test]
fn a_child_replaces_blocks() {
    let child = r#"{% extends "base" %}
{% block title %}{{ page.title }} | {{ super() }}{% endblock %}
{% block body %}<h1>{{ page.title }}</h1>{% endblock %}"#;
    assert_eq!(
        render(&[("base", BASE), ("home", child)], "home"),
        "<html><head><title>News | Site</title></head>\n<body><h1>News</h1><footer>(c)</footer></body></html>"
    );
    // The base renders on its own with its defaults.
    assert!(render(&[("base", BASE)], "base").contains("<p>empty</p>"));
}

#[test]
fn chains_and_nested_blocks() {
    let base = "[{% block outer %}o({% block inner %}i{% endblock %}){% endblock %}]";
    let mid = r#"{% extends "base" %}{% block inner %}mid-{{ super() }}{% endblock %}"#;
    let leaf = r#"{% extends "mid" %}{% block inner %}leaf-{{ super() }}{% endblock %}"#;
    assert_eq!(
        render(&[("base", base), ("mid", mid), ("leaf", leaf)], "leaf"),
        "[o(leaf-mid-i)]"
    );
    let over = r#"{% extends "mid" %}{% block outer %}X{% endblock %}"#;
    assert_eq!(
        render(&[("base", base), ("mid", mid), ("over", over)], "over"),
        "[X]"
    );
}

#[test]
fn blocks_see_loops_and_contexts() {
    let base = r#"<ul>{% for p in page.posts %}<li title="{% block tip %}{% endblock %}">{% block item %}{% endblock %}</li>{% endfor %}</ul>"#;
    let child = r#"{% extends "base" %}{% block tip %}{{ p.author.name }}{% endblock %}{% block item %}{{ loop.index }}. {{ p.title }}{% endblock %}"#;
    assert_eq!(
        render(&[("base", base), ("list", child)], "list"),
        r#"<ul><li title="Ann">1. One</li><li title="Bo">2. Two &amp; more</li></ul>"#
    );
}

#[test]
fn layout_errors() {
    for (templates, want) in [
        (
            vec![("c", r#"{% extends "nope" %}"#)],
            "unknown template `nope`",
        ),
        (
            vec![("a", r#"{% extends "b" %}"#), ("b", r#"{% extends "a" %}"#)],
            "extends itself",
        ),
        (
            vec![
                ("base", BASE),
                (
                    "c",
                    r#"{% extends "base" %}{% block titel %}x{% endblock %}"#,
                ),
            ],
            "did you mean `title`",
        ),
        (
            vec![("base", BASE), ("c", r#"{% extends "base" %}hello"#)],
            "holds only blocks",
        ),
        (
            vec![
                ("base", BASE),
                ("c", r#"{% extends "base" %}{{ page.title }}"#),
            ],
            "holds only blocks",
        ),
        (
            vec![("base", BASE), ("c", r#"x{% extends "base" %}"#)],
            "must be the first",
        ),
        (
            vec![(
                "c",
                "{% block a %}{% endblock %}{% block a %}{% endblock %}",
            )],
            "defined twice",
        ),
        (
            vec![("c", "{{ super() }}")],
            "only available inside a block",
        ),
        (
            vec![("c", "{% block a %}{{ super() }}{% endblock %}")],
            "no parent version",
        ),
        (
            vec![("c", "{% block a %}x{% endblock b %}")],
            "closes `block a`",
        ),
        (
            vec![
                ("base", BASE),
                (
                    "c",
                    r#"{% extends "base" %}{% block body %}{{ page.nope }}{% endblock %}"#,
                ),
            ],
            "c:1:",
        ),
    ] {
        let e = error(&templates);
        assert!(e.contains(want), "{want}: {e}");
    }
}

#[test]
fn self_containing_blocks_are_refused() {
    // Through `super()`, the leaf's `b` reaches the middle's `b`, which holds
    // block `a`, which is the leaf's `a`, which holds `b` again.
    let base = "{% block a %}A{% block b %}B{% endblock %}{% endblock %}";
    let mid = r#"{% extends "base" %}{% block b %}{% block a %}M{% endblock %}{% endblock %}"#;
    let leaf =
        r#"{% extends "mid" %}{% block a %}{% block b %}{{ super() }}{% endblock %}{% endblock %}"#;
    let e = error(&[("base", base), ("mid", mid), ("leaf", leaf)]);
    assert!(e.contains("inside itself"), "{e}");
}

#[test]
fn include_with_gives_a_partial_its_own_root() {
    let card = Schema::build("post")
        .text("title")
        .object("author", Schema::build("author").text("name").finish())
        .finish();
    let engine = Engine::<Page>::builder()
        .template(
            "list",
            r#"{% for p in page.posts %}{% include "card" with p %}{% endfor %}"#,
        )
        .template_with_schema(
            "card",
            r#"<b title="{{ post.author.name }}">{{ post.title }}</b>"#,
            card,
        )
        .build()
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        engine.render("list", &page()).unwrap(),
        r#"<b title="Ann">One</b><b title="Bo">Two &amp; more</b>"#
    );

    // The partial is checked against the value it is given.
    let err = Engine::<Page>::builder()
        .template(
            "list",
            r#"{% for p in page.posts %}{% include "card" with p.author %}{% endfor %}"#,
        )
        .template_with_schema(
            "card",
            "{{ post.title }}",
            Schema::build("post").text("title").finish(),
        )
        .build()
        .unwrap_err()
        .to_string();
    assert!(err.contains("has no field `title`"), "{err}");
    let err = error(&[("a", r#"{% include "b" with page.title %}"#), ("b", "")]);
    assert!(err.contains("needs an object"), "{err}");
}

#[test]
fn templates_from_a_folder() {
    let dir = std::env::temp_dir().join(format!("luminess-dir-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("partials")).unwrap();
    std::fs::write(dir.join("base.html"), BASE).unwrap();
    std::fs::write(
        dir.join("home.html"),
        r#"{% extends "base" %}{% block body %}{% include "partials/nav" %}{% endblock %}"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("partials").join("nav.html"),
        "<nav>{{ page.title }}</nav>",
    )
    .unwrap();
    std::fs::write(dir.join("notes.txt"), "ignored").unwrap();
    let engine = Engine::<Page>::builder()
        .directory(&dir)
        .unwrap()
        .build()
        .unwrap_or_else(|e| panic!("{e}"));
    // Named by path without the extension; notes.txt is not a template.
    for name in ["base", "home", "partials/nav"] {
        assert!(engine.render(name, &page()).is_ok(), "{name}");
    }
    assert!(engine.render("notes", &page()).is_err());
    assert!(
        engine
            .render("home", &page())
            .unwrap()
            .contains("<nav>News</nav>")
    );
    assert!(
        Engine::<Page>::builder()
            .directory(dir.join("missing"))
            .is_err()
    );
    let _ = std::fs::remove_dir_all(&dir);
}
