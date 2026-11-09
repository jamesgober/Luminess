//! Behaviour of whole templates: output, escaping per context, control flow,
//! blocks with fallbacks, includes, and reload.

use std::fmt::Write as _;

use luminess::{Engine, Failed, FieldId, Kind, Output, Schema, Shaped, ToValue, Value, View};

struct Author {
    name: String,
}

struct Post {
    title: String,
    url: String,
    tags: Vec<String>,
}

struct Gallery {
    images: Vec<String>,
    broken: bool,
}

struct Page {
    title: String,
    body: String,
    subtitle: Option<String>,
    views: u32,
    rating: f64,
    featured: bool,
    author: Author,
    posts: Vec<Post>,
    gallery: Gallery,
}

luminess::view!(Author as "author" { name: text });
luminess::view!(Post as "post" { title: text, url: text, tags: list(text) });
luminess::view!(Page as "page" {
    title: text,
    body: html,
    subtitle: text,
    views: int,
    rating: float,
    featured: bool,
    author: object(Author),
    posts: list(object(Post)),
    gallery: object(Gallery),
});

impl Shaped for Gallery {
    fn schema() -> Schema {
        Schema::build("gallery").int("count").renders().finish()
    }
}

impl View for Gallery {
    fn field(&self, id: FieldId) -> Value<'_> {
        match id.index() {
            0 => Value::Int(self.images.len() as i64),
            _ => Value::Absent,
        }
    }

    fn render(&self, out: &mut Output<'_>) -> Result<(), Failed> {
        out.markup("<div class=\"gallery\">");
        for img in &self.images {
            out.markup("<img src=\"");
            out.url(img);
            out.markup("\">");
        }
        if self.broken {
            return Err(Failed::new("image service down"));
        }
        out.markup("</div>");
        Ok(())
    }

    fn fallback(&self, out: &mut Output<'_>) {
        let _ = write!(out, "{} images unavailable", self.images.len());
    }

    fn text(&self, out: &mut String) -> Result<(), Failed> {
        if self.broken {
            return Err(Failed::new("image service down"));
        }
        let _ = write!(out, "{} images & more", self.images.len());
        Ok(())
    }
}

impl ToValue for Gallery {
    fn to_value(&self) -> Value<'_> {
        Value::View(self)
    }
}

fn page() -> Page {
    Page {
        title: "Tom & \"Jerry\" <3".into(),
        body: "<p>Hello <b>world</b></p>".into(),
        subtitle: None,
        views: 1234,
        rating: 4.5,
        featured: true,
        author: Author {
            name: "Ann O'Neil".into(),
        },
        posts: vec![
            Post {
                title: "First".into(),
                url: "/first".into(),
                tags: vec!["a".into(), "b".into()],
            },
            Post {
                title: "Second".into(),
                url: "javascript:alert(1)".into(),
                tags: vec![],
            },
        ],
        gallery: Gallery {
            images: vec!["/a.png".into()],
            broken: false,
        },
    }
}

fn render(src: &str) -> String {
    render_with(src, &page())
}

fn render_with(src: &str, p: &Page) -> String {
    let engine = Engine::<Page>::builder()
        .template("t", src)
        .build()
        .unwrap_or_else(|e| panic!("{e}"));
    engine.render_to_string("t", p).unwrap()
}

fn compile_error(src: &str) -> String {
    match Engine::<Page>::builder().template("t", src).build() {
        Ok(_) => panic!("expected a compile error for {src:?}"),
        Err(e) => e.diagnostics()[0].to_string(),
    }
}

#[test]
fn text_and_numbers() {
    assert_eq!(
        render("<h1>{{ page.title }}</h1>"),
        "<h1>Tom &amp; &quot;Jerry&quot; &lt;3</h1>"
    );
    assert_eq!(
        render("{{ page.views }} {{ page.rating }} {{ page.featured }}"),
        "1234 4.5 true"
    );
    assert_eq!(render("[{{ page.subtitle }}]"), "[]");
    assert_eq!(render("{{ page.author.name }}"), "Ann O&#39;Neil");
    assert_eq!(render("plain"), "plain");
    assert_eq!(render(""), "");
}

#[test]
fn trusted_markup() {
    assert_eq!(render("{{ page.body }}"), "<p>Hello <b>world</b></p>");
    assert_eq!(
        render("<p title=\"{{ page.body }}\">"),
        "<p title=\"&lt;p&gt;Hello &lt;b&gt;world&lt;/b&gt;&lt;/p&gt;\">"
    );
    assert_eq!(render("{{ page.title | raw }}"), "Tom & \"Jerry\" <3");
}

#[test]
fn attributes() {
    assert_eq!(
        render("<a title=\"{{ page.title }}\">"),
        "<a title=\"Tom &amp; &quot;Jerry&quot; &lt;3\">"
    );
    assert_eq!(
        render("<a title='{{ page.author.name }}'>"),
        "<a title='Ann O&#39;Neil'>"
    );
    assert_eq!(
        render("<title>{{ page.title }}</title>"),
        "<title>Tom &amp; &quot;Jerry&quot; &lt;3</title>"
    );
}

#[test]
fn urls() {
    let src = "{% for p in page.posts %}<a href=\"{{ p.url }}\">{% endfor %}";
    assert_eq!(render(src), "<a href=\"/first\"><a href=\"#blocked\">");
    assert_eq!(
        render("<a href=\"/search?q={{ page.title }}\">"),
        "<a href=\"/search?q=Tom%20%26%20%22Jerry%22%20%3C3\">"
    );
    assert_eq!(
        render("<a href=\"/by/{{ page.author.name }}\">"),
        "<a href=\"/by/Ann%20O%27Neil\">"
    );
}

#[test]
fn conditions() {
    assert_eq!(render("{% if page.featured %}F{% else %}N{% endif %}"), "F");
    assert_eq!(
        render("{% if page.subtitle %}S{% elif page.views > 1000 %}V{% endif %}"),
        "V"
    );
    assert_eq!(
        render("{% if not page.subtitle and page.rating >= 4.5 %}y{% endif %}"),
        "y"
    );
    assert_eq!(
        render("{% if page.title == \"x\" or page.views != 1234 %}y{% else %}n{% endif %}"),
        "n"
    );
    assert_eq!(
        render("{% if page.subtitle == \"x\" %}y{% else %}n{% endif %}"),
        "n"
    );
    assert_eq!(render("{% if page.subtitle != \"x\" %}y{% endif %}"), "y");
    assert_eq!(render("{% if page.posts %}has{% endif %}"), "has");
    assert_eq!(
        render("{% if (page.views < 2) or (page.rating > 1) %}y{% endif %}"),
        "y"
    );
}

#[test]
fn loops() {
    let src = "{% for p in page.posts %}{{ loop.index }}/{{ loop.length }}:{{ p.title }}{% if loop.last %}.{% else %},{% endif %}{% endfor %}";
    assert_eq!(render(src), "1/2:First,2/2:Second.");
    let nested = "{% for p in page.posts %}[{% for t in p.tags %}{{ t }}{% if not loop.last %} {% endif %}{% else %}none{% endfor %}]{% endfor %}";
    assert_eq!(render(nested), "[a b][none]");
    let mut p = page();
    p.posts.clear();
    assert_eq!(
        render_with("{% for x in page.posts %}x{% else %}empty{% endfor %}", &p),
        "empty"
    );
    assert_eq!(
        render("{{ page.posts | length }} {{ page.title | length }}"),
        "2 16"
    );
    assert_eq!(
        render(
            "{% for p in page.posts %}{{ loop.index0 }}{{ loop.revindex }}{{ loop.first }} {% endfor %}"
        ),
        "02true 11false "
    );
}

#[test]
fn filters() {
    assert_eq!(render("{{ page.subtitle | default(\"none\") }}"), "none");
    assert_eq!(render("{{ page.author.name | upper }}"), "ANN O&#39;NEIL");
    assert_eq!(
        render("{{ page.author.name | lower | default('x') }}"),
        "ann o&#39;neil"
    );
    assert_eq!(render("[{{ page.subtitle | upper }}]"), "[]");
}

#[test]
fn whitespace_comments_raw() {
    assert_eq!(
        render(
            "<ul>\n  {%- for p in page.posts %}\n  <li>{{ p.title }}</li>\n  {%- endfor %}\n</ul>"
        ),
        "<ul>\n  <li>First</li>\n  <li>Second</li>\n</ul>"
    );
    assert_eq!(render("a{# nothing #}b"), "ab");
    assert_eq!(
        render("{% raw %}{{ page.nope }}{% endraw %}"),
        "{{ page.nope }}"
    );
}

#[test]
fn blocks_render_themselves() {
    assert_eq!(
        render("{{ page.gallery }}"),
        "<div class=\"gallery\"><img src=\"/a.png\"></div>"
    );
    assert_eq!(render("{{ page.gallery.count }}"), "1");
}

#[test]
fn failing_blocks_fall_back() {
    let mut p = page();
    p.gallery.broken = true;
    let engine = Engine::<Page>::builder()
        .template("a", "<main>{{ page.gallery }}</main>")
        .template("b", "<main>{% try %}<h2>Pictures</h2>{{ page.gallery }}{% fallback %}<p>Later.</p>{% endtry %}</main>")
        .build()
        .unwrap();
    let mut out = Vec::new();
    let r = engine.render("a", &p, &mut out).unwrap();
    assert_eq!(
        String::from_utf8(out).unwrap(),
        "<main>1 images unavailable</main>"
    );
    assert_eq!(r.fallbacks(), 1);
    assert!(!r.is_complete());
    assert_eq!(
        r.first_failure().map(Failed::reason),
        Some("image service down")
    );
    assert_eq!(
        engine.render_to_string("b", &p).unwrap(),
        "<main><p>Later.</p></main>"
    );
    p.gallery.broken = false;
    assert_eq!(
        engine.render_to_string("b", &p).unwrap(),
        "<main><h2>Pictures</h2><div class=\"gallery\"><img src=\"/a.png\"></div></main>"
    );
}

#[test]
fn includes() {
    let engine = Engine::<Page>::builder()
        .template("nav", "<nav>{{ page.author.name }}</nav>")
        .template("item", "<li>{{ p.title }}</li>")
        .template("home", "{% include \"nav\" %}<ul>{% for p in page.posts %}{% include \"item\" %}{% endfor %}</ul>")
        .template_with("item", "x", Schema::build("p").text("title").finish());
    // Two templates named `item`.
    assert!(
        engine
            .build()
            .unwrap_err()
            .to_string()
            .contains("defined twice")
    );

    let engine = Engine::<Page>::builder()
        .template("nav", "<nav>{{ page.author.name }}</nav>")
        .template("home", "{% include \"nav\" %}<ul>{% for p in page.posts %}{% include \"item\" %}{% endfor %}</ul>")
        .template_with("item", "<li>{{ p.title }}</li>", Schema::build("p").text("title").finish())
        .build()
        .unwrap();
    assert_eq!(
        engine.render_to_string("home", &page()).unwrap(),
        "<nav>Ann O&#39;Neil</nav><ul><li>First</li><li>Second</li></ul>"
    );
}

#[test]
fn include_errors() {
    let err = Engine::<Page>::builder()
        .template("a", "{% include \"b\" %}")
        .template("b", "{% include \"a\" %}")
        .build()
        .unwrap_err();
    assert!(err.to_string().contains("includes itself"), "{err}");
    let err = Engine::<Page>::builder()
        .template("a", "{% include \"nva\" %}")
        .template("nav", "")
        .build()
        .unwrap_err();
    assert!(err.to_string().contains("did you mean `nav`"), "{err}");
    let err = Engine::<Page>::builder()
        .template("a", "<p title=\"{% include \"b\" %}\">")
        .template("b", "{{ page.nope }}")
        .build()
        .unwrap_err();
    assert!(err.to_string().contains("b:1:9"), "{err}");
}

#[test]
fn unknown_names_fail_at_compile_time() {
    assert_eq!(
        compile_error("{{ page.titel }}"),
        "t:1:9: `page` (page) has no field `titel`, did you mean `title`?"
    );
    assert_eq!(
        compile_error("x\n  {{ pgae.title }}"),
        "t:2:6: unknown name `pgae`, did you mean `page`?; names here: `page`"
    );
    assert!(compile_error("{{ page.title | uper }}").contains("did you mean `upper`"));
    assert!(compile_error("{{ page.title.x }}").contains("is text and has no fields"));
    assert!(compile_error("{{ page.author }}").contains("write one of its fields (name)"));
    assert!(compile_error("{{ page.posts }}").contains("loop over it"));
    assert!(compile_error("{% for x in page.title %}{% endfor %}").contains("needs a list"));
    assert!(compile_error("{{ loop.index }}").contains("only available inside a `for`"));
    assert!(compile_error("{% if page.views == \"x\" %}{% endif %}").contains("cannot compare"));
    assert!(compile_error("{{ page.views | upper }}").contains("applies to text"));
    assert!(compile_error("{% for page in page.posts %}{% endfor %}").contains("root value"));
}

#[test]
fn unsafe_places_fail_at_compile_time() {
    for (src, want) in [
        ("<a href={{ page.title }}>", "unquoted"),
        ("<a {{ page.title }}>", "attribute name"),
        ("<{{ page.title }}>", "tag name"),
        ("<!-- {{ page.title }} -->", "comment"),
        (
            "<p onclick=\"f(&quot;{{ page.title }}&quot;)\">",
            "character references",
        ),
        (
            "<p style=\"x: &quot;{{ page.title }}\">",
            "character references",
        ),
        (
            "<script>// {{ page.title }}
</script>",
            "JavaScript comment",
        ),
        (
            "<script>/* {{ page.title }} */</script>",
            "JavaScript comment",
        ),
        (
            "<script>var r = /a{{ page.title }}/;</script>",
            "regular expression",
        ),
        ("<script>var r = 1 /{{ page.title }};</script>", "after `/`"),
        ("<script>var s = `${{ page.title }}`;</script>", "after `$`"),
        ("<style>/* {{ page.title }} */</style>", "CSS comment"),
        ("<script>var a = 1;", "ends inside JavaScript code"),
        ("<style>a {", "ends inside a style element"),
        ("<p title=\"x", "ends inside an attribute value"),
        ("<!-- x", "ends inside an HTML comment"),
        ("<iframe srcdoc=\"{{ page.title }}\">", "srcdoc"),
        ("<p title=\"{{ page.title | raw }}\">", "HTML text only"),
        (
            "<a {% if page.featured %}title=\"{% endif %}{{ page.title }}\">",
            "different places",
        ),
        (
            "{% for p in page.posts %}<a title=\"{% endfor %}",
            "starts in HTML text",
        ),
    ] {
        let e = compile_error(src);
        assert!(e.contains(want), "{src}: {e}");
    }
}

#[test]
fn every_error_is_reported() {
    let err = Engine::<Page>::builder()
        .template("a", "{{ page.x }} {{ page.y }}")
        .template("b", "{% if %}")
        .build()
        .unwrap_err();
    assert_eq!(err.diagnostics().len(), 3, "{err}");
    assert!(err.to_string().starts_with("3 template errors"));
}

#[test]
fn branches_that_join() {
    // Both branches stay inside the same URL value: the output is checked as
    // a URL start, the stricter of the two.
    assert_eq!(
        render("<a href=\"{% if page.featured %}/f/{% endif %}{{ page.title }}\">"),
        "<a href=\"/f/Tom%20&amp;%20%22Jerry%22%20%3C3\">"
    );
    assert_eq!(
        render("<p {% if page.featured %}hidden{% endif %}>x</p>"),
        "<p hidden>x</p>"
    );
}

#[test]
fn reload_and_snapshots() {
    let engine = Engine::<Page>::builder()
        .template("t", "one")
        .build()
        .unwrap();
    let old = engine.templates();
    engine
        .reload(Engine::<Page>::builder().template("t", "two"))
        .unwrap();
    assert_eq!(old.render_to_string("t", &page()).unwrap(), "one");
    assert_eq!(engine.render_to_string("t", &page()).unwrap(), "two");
    assert!(
        engine
            .reload(Engine::<Page>::builder().template("t", "{{ x }}"))
            .is_err()
    );
    assert_eq!(engine.render_to_string("t", &page()).unwrap(), "two");
    let e = engine.render_to_string("missing", &page()).unwrap_err();
    assert_eq!(e.kind(), luminess::RenderErrorKind::NotFound);
    assert_eq!(e.to_string(), "no template named `missing`");
}

#[test]
fn engines_cross_threads() {
    let engine = Engine::<Page>::builder()
        .template("t", "{{ page.views }}")
        .build()
        .unwrap();
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let e = engine.clone();
            std::thread::spawn(move || {
                let p = page();
                for _ in 0..100 {
                    assert_eq!(e.render_to_string("t", &p).unwrap(), "1234");
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
}

#[test]
fn runtime_schemas() {
    // A content type declared at startup rather than in Rust types.
    struct Doc(Vec<String>);
    impl View for Doc {
        fn field(&self, id: FieldId) -> Value<'_> {
            self.0
                .get(id.index())
                .map_or(Value::Absent, |s| Value::Text(s))
        }
    }
    let schema = Schema::build("doc").text("summary").text("sku").finish();
    let engine = Engine::<Doc>::builder_with(schema.clone())
        .template("product", "{{ doc.summary }} ({{ doc.sku }})")
        .build()
        .unwrap();
    let d = Doc(vec!["Shoes".into(), "S-1".into()]);
    assert_eq!(
        engine.render_to_string("product", &d).unwrap(),
        "Shoes (S-1)"
    );
    let lists = Schema::build("doc").list("tags", Kind::Text).finish();
    assert!(
        Engine::<Doc>::builder_with(lists)
            .template("x", "{{ doc.tags }}")
            .build()
            .is_err()
    );
}

#[test]
fn render_appends() {
    let engine = Engine::<Page>::builder()
        .template("t", "b")
        .build()
        .unwrap();
    let mut out = b"a".to_vec();
    let r = engine.render("t", &page(), &mut out).unwrap();
    assert_eq!(out, b"ab");
    assert_eq!(r.bytes(), 1);
}

#[test]
fn scripts() {
    assert_eq!(
        render("<script>var t = {{ page.title }};</script>"),
        r#"<script>var t = "Tom \u0026 \u0022Jerry\u0022 \u003C3";</script>"#
    );
    assert_eq!(
        render("<script>var t = '{{ page.author.name }}';</script>"),
        r"<script>var t = 'Ann O\u0027Neil';</script>"
    );
    assert_eq!(
        render(
            "<script>var n = {{ page.views }}, r = {{ page.rating }}, f = {{ page.featured }}, s = {{ page.subtitle }};</script>"
        ),
        "<script>var n = 1234, r = 4.5, f = true, s = null;</script>"
    );
    assert_eq!(
        render("<script>var a = `{{ page.title | upper }}`;</script>"),
        r"<script>var a = `TOM \u0026 \u0022JERRY\u0022 \u003C3`;</script>"
    );
    assert_eq!(
        render("<script>if (/x/.test(s)) { go({{ page.views }}); }</script>"),
        "<script>if (/x/.test(s)) { go(1234); }</script>"
    );
    assert_eq!(
        render("<button onclick=\"go({{ page.author.name }})\">"),
        r#"<button onclick="go(&quot;Ann O\u0027Neil&quot;)">"#
    );
    assert_eq!(
        render("<button onclick=\"go('{{ page.author.name }}')\">"),
        r#"<button onclick="go('Ann O\u0027Neil')">"#
    );
    assert_eq!(
        render("<button onclick=\"a && go({{ page.views }})\">"),
        "<button onclick=\"a && go(1234)\">"
    );
}

#[test]
fn styles() {
    #[allow(clippy::items_after_statements)]
    struct Theme {
        color: String,
        font: String,
    }
    luminess::view!(Theme as "theme" { color: text, font: text });
    let engine = Engine::<Theme>::builder()
        .template(
            "css",
            "<style>a { color: {{ theme.color }}; font-family: '{{ theme.font }}'; }</style>",
        )
        .template("attr", "<p style=\"color: {{ theme.color }}\">")
        .build()
        .unwrap();
    let ok = Theme {
        color: "#ff0000".into(),
        font: "Open Sans".into(),
    };
    assert_eq!(
        engine.render_to_string("css", &ok).unwrap(),
        r"<style>a { color: #ff0000; font-family: 'Open\20 Sans'; }</style>"
    );
    let bad = Theme {
        color: "red; background: url(x)".into(),
        font: "a'}</style>".into(),
    };
    assert_eq!(
        engine.render_to_string("css", &bad).unwrap(),
        r"<style>a { color: blocked; font-family: 'a\27 \7d \3c \2f style\3e '; }</style>"
    );
    assert_eq!(
        engine.render_to_string("attr", &bad).unwrap(),
        "<p style=\"color: blocked\">"
    );
}

#[test]
fn block_fallback_text_and_text_forms() {
    let mut p = page();
    let engine = Engine::<Page>::builder()
        .template("f", r#"{{ page.gallery | fallback("No pictures <now>") }}"#)
        .template(
            "attr",
            r#"<div title="{{ page.gallery }}" data-n='{{ page.gallery | fallback("none") }}'>"#,
        )
        .template("js", "<script>var g = {{ page.gallery }};</script>")
        .template(
            "many",
            "{% for x in page.posts %}{{ page.gallery }}{% endfor %}",
        )
        .build()
        .unwrap();
    assert_eq!(
        engine.render_to_string("attr", &p).unwrap(),
        r#"<div title="1 images &amp; more" data-n='1 images &amp; more'>"#
    );
    assert_eq!(
        engine.render_to_string("js", &p).unwrap(),
        r#"<script>var g = "1 images \u0026 more";</script>"#
    );
    p.gallery.broken = true;
    assert_eq!(
        engine.render_to_string("f", &p).unwrap(),
        "No pictures &lt;now&gt;"
    );
    assert_eq!(
        engine.render_to_string("attr", &p).unwrap(),
        r#"<div title="" data-n='none'>"#
    );
    let mut out = Vec::new();
    let r = engine.render("many", &p, &mut out).unwrap();
    assert_eq!(r.fallbacks(), 2);
    assert_eq!(r.failures().len(), 2);
    assert!(
        r.failures()
            .iter()
            .all(|f| f.reason() == "image service down")
    );
    assert!(compile_error(r#"{{ page.title | fallback("x") }}"#).contains("use `default`"));
    assert!(compile_error("{{ page.gallery | fallback(1) }}").contains("takes one string"));
}
