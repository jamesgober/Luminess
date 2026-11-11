<h1 align="center">
    <img width="99" alt="Rust logo" src="https://raw.githubusercontent.com/jamesgober/rust-collection/72baabd71f00e14aa9184efcb16fa3deddda3a0a/assets/rust-logo.svg">
    <br><b>Luminess</b><br>
    <sub><sup>API REFERENCE</sup></sub>
</h1>
<div align="center">
    <sup>
        <a href="../README.md" title="Project Home"><b>HOME</b></a>
        <span>&nbsp;│&nbsp;</span>
        <span>API</span>
        <span>&nbsp;│&nbsp;</span>
        <a href="./SYNTAX.md" title="Template syntax"><b>SYNTAX</b></a>
        <span>&nbsp;│&nbsp;</span>
        <a href="./SECURITY.md" title="Security model"><b>SECURITY</b></a>
        <span>&nbsp;│&nbsp;</span>
        <a href="./PERFORMANCE.md" title="Performance"><b>PERFORMANCE</b></a>
    </sup>
</div>
<br>

A task-oriented tour of the 0.10 API, simplest first. The rustdoc on
[docs.rs](https://docs.rs/luminess) is the exhaustive reference; every example
here is a complete program, compiled and run as a doctest.

- [Tier 1: the common case](#tier-1-the-common-case)
  - [Describing a view: `view!`](#describing-a-view-view)
  - [Loading a theme: `Engine::from_dir`](#loading-a-theme-enginefrom_dir)
  - [Rendering: `Engine::render`](#rendering-enginerender)
  - [Reloading: `Engine::reload_dir`](#reloading-enginereload_dir)
  - [One template: `Template`](#one-template-template)
  - [One render: `luminess::render`](#one-render-luminessrender)
  - [Errors: `Error`, `ErrorKind`, `Diagnostic`](#errors-error-errorkind-diagnostic)
- [Tier 2: more control](#tier-2-more-control)
  - [Building a theme: `Engine::builder` and `Builder`](#building-a-theme-enginebuilder-and-builder)
  - [Reusing a buffer: `render_into` and `Rendered`](#reusing-a-buffer-render_into-and-rendered)
  - [Swapping in a theme: `Engine::replace` and `Engine::contains`](#swapping-in-a-theme-enginereplace-and-enginecontains)
  - [Your own filters: `Builder::filter`](#your-own-filters-builderfilter)
  - [web-net: `web::Theme` and `web::ErrorView`](#web-net-webtheme-and-weberrorview)
- [Tier 3: the view layer](#tier-3-the-view-layer)
  - [`View`](#view)
  - [`Value`, `List`, and `ToValue`](#value-list-and-tovalue)
  - [Methods: `View::call` and `Answer`](#methods-viewcall-and-answer)
  - [Blocks that render themselves: `Output` and `BlockError`](#blocks-that-render-themselves-output-and-blockerror)
  - [Schemas: `Schema`, `SchemaBuilder`, `Kind`, `HasSchema`](#schemas-schema-schemabuilder-kind-hasschema)
  - [Schemas built at startup: `Engine::with_schema`](#schemas-built-at-startup-enginewith_schema)
  - [Escapers: `luminess::escape`](#escapers-luminessescape)
- [Why these names](#why-these-names)

## Setup

```toml
[dependencies]
luminess = "0.10"
# with the web-net integration:
# luminess = { version = "0.10", features = ["web-net"] }
```

Everything public is at the crate root, plus the `escape` module, the
`view!` macro, and (with the `web-net` feature) the `web` module.

---

# Tier 1: the common case

## Describing a view: `view!`

```text
luminess::view!(Type as "name" { field: kind, ... });
```

Declares what templates may read from a struct: each field, with its kind.
Templates refer to the value by `name` (`{{ page.title }}`), and every name in
every template is checked against these fields when the theme is compiled.

**Kinds:** `text`, `html` (trusted markup, written as is in HTML text), `int`,
`float`, `bool`, `object(Type)` (another struct with its own `view!`), and
`list(kind)`. A field of type `Option<T>` is absent when it is `None`.

**Generates:** `View`, `HasSchema`, and `ToValue` for the struct.

```rust
use luminess::Template;

struct Post { title: String, url: String }
struct Page { title: String, posts: Vec<Post>, subtitle: Option<String> }

luminess::view!(Post as "post" { title: text, url: text });
luminess::view!(Page as "page" { title: text, posts: list(object(Post)), subtitle: text });

fn main() -> Result<(), luminess::Error> {
    let t = Template::<Page>::new(
        "<h1>{{ page.title }}</h1>{{ page.subtitle | default(\"-\") }}\
         {% for p in page.posts %}<a href=\"{{ p.url }}\">{{ p.title }}</a>{% endfor %}",
    )?;
    let page = Page {
        title: "News & notes".into(),
        posts: vec![Post { title: "First".into(), url: "/first".into() }],
        subtitle: None,
    };
    assert_eq!(t.render(&page)?, "<h1>News &amp; notes</h1>-<a href=\"/first\">First</a>");
    Ok(())
}
```

A misspelled field is caught before anything renders:

```rust
struct Page { title: String }
luminess::view!(Page as "page" { title: text });

fn main() {
    let err = luminess::Template::<Page>::new("{{ page.titel }}").unwrap_err();
    assert!(err.to_string().contains("did you mean `title`"));
}
```

## Loading a theme: `Engine::from_dir`

```text
Engine::<V>::from_dir(dir: impl AsRef<Path>) -> Result<Engine<V>, Error>
```

Loads every `.html` file below `dir` as a template and compiles them all
against `V`. A template is named by its path below `dir`, with `/` between
folders and without the extension: `partials/nav.html` is `partials/nav`.
Templates refer to each other by those names in `include` and `extends`.

**Parameters:** `dir`, the theme's folder.

**Errors:** `ErrorKind::Io` when the folder or a file cannot be read (the
message names the path); `ErrorKind::Compile` with every problem in every
template.

```rust
use luminess::Engine;

struct Page { title: String }
luminess::view!(Page as "page" { title: text });

fn main() -> Result<(), luminess::Error> {
    let dir = std::env::temp_dir().join(format!("luminess-api-dir-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("partials")).unwrap();
    std::fs::write(dir.join("home.html"), "<h1>{{ page.title }}</h1>{% include \"partials/nav\" %}").unwrap();
    std::fs::write(dir.join("partials/nav.html"), "<nav>menu</nav>").unwrap();

    let engine = Engine::<Page>::from_dir(&dir)?;
    assert_eq!(engine.render("home", &Page { title: "Hi".into() })?, "<h1>Hi</h1><nav>menu</nav>");

    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}
```

A missing folder is an `Io` error, a mistake in a template a `Compile` error:

```rust
use luminess::{Engine, ErrorKind};

struct Page { title: String }
luminess::view!(Page as "page" { title: text });

fn main() {
    let err = Engine::<Page>::from_dir("no/such/theme").unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Io);
}
```

## Rendering: `Engine::render`

```text
engine.render(name: &str, view: &V) -> Result<String, Error>
```

Renders the template called `name` for `view` and returns the page.

**Parameters:** `name`, a template name; `view`, the value the template reads.

**Errors:** `ErrorKind::NotFound` when no template has that name;
`ErrorKind::TooLarge` past a limit set with `Builder::max_output`.

```rust
use luminess::{Engine, ErrorKind};

struct Page { title: String }
luminess::view!(Page as "page" { title: text });

fn main() -> Result<(), luminess::Error> {
    let engine = Engine::<Page>::builder().template("home", "<title>{{ page.title }}</title>").build()?;
    assert_eq!(engine.render("home", &Page { title: "<b>".into() })?, "<title>&lt;b&gt;</title>");
    assert_eq!(engine.render("missing", &Page { title: String::new() }).unwrap_err().kind(), ErrorKind::NotFound);
    Ok(())
}
```

An `Engine` is cheap to clone and safe to share across threads:

```rust
use luminess::Engine;

struct Page { n: i64 }
luminess::view!(Page as "page" { n: int });

fn main() -> Result<(), luminess::Error> {
    let engine = Engine::<Page>::builder().template("t", "{{ page.n }}").build()?;
    let handles: Vec<_> = (0..4)
        .map(|i| {
            let e = engine.clone();
            std::thread::spawn(move || e.render("t", &Page { n: i }).unwrap())
        })
        .collect();
    for (i, h) in handles.into_iter().enumerate() {
        assert_eq!(h.join().unwrap(), i.to_string());
    }
    Ok(())
}
```

## Reloading: `Engine::reload_dir`

```text
engine.reload_dir(dir: impl AsRef<Path>) -> Result<(), Error>
```

Compiles the templates in `dir` again and swaps them in for every clone of
the engine. Renders in flight finish on the theme they started with. On any
error the running theme stays, so a typo in a live theme never takes the
site down.

**Errors:** as `Engine::from_dir`.

```rust
use luminess::Engine;

struct Page { title: String }
luminess::view!(Page as "page" { title: text });

fn main() -> Result<(), luminess::Error> {
    let dir = std::env::temp_dir().join(format!("luminess-api-reload-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("home.html"), "old {{ page.title }}").unwrap();
    let engine = Engine::<Page>::from_dir(&dir)?;

    std::fs::write(dir.join("home.html"), "new {{ page.title }}").unwrap();
    engine.reload_dir(&dir)?;
    assert_eq!(engine.render("home", &Page { title: "x".into() })?, "new x");

    // A broken edit is refused; the site keeps serving the last good theme.
    std::fs::write(dir.join("home.html"), "{{ page.tilte }}").unwrap();
    assert!(engine.reload_dir(&dir).is_err());
    assert_eq!(engine.render("home", &Page { title: "x".into() })?, "new x");

    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}
```

## One template: `Template`

```text
Template::<V>::new(source: impl Into<String>) -> Result<Template<V>, Error>
template.render(view: &V) -> Result<String, Error>
template.render_into(view: &V, out: &mut Vec<u8>) -> Result<Rendered, Error>
```

One compiled template, for a card, an email, or any fragment rendered on its
own. Compile it once and render it as often as needed; clones share the
compiled template.

**Errors:** `new` returns `ErrorKind::Compile` with every problem; `render`
cannot fail in practice (the `Result` keeps every render call the same shape).

```rust
use luminess::Template;

struct Card { title: String, url: String }
luminess::view!(Card as "card" { title: text, url: text });

fn main() -> Result<(), luminess::Error> {
    let card = Template::<Card>::new(r#"<a href="{{ card.url }}">{{ card.title }}</a>"#)?;
    let a = card.render(&Card { title: "Docs".into(), url: "/docs".into() })?;
    let b = card.render(&Card { title: "Bad".into(), url: "javascript:alert(1)".into() })?;
    assert_eq!(a, r#"<a href="/docs">Docs</a>"#);
    assert_eq!(b, r##"<a href="#blocked">Bad</a>"##);
    Ok(())
}
```

## One render: `luminess::render`

```text
luminess::render(source: &str, view: &V) -> Result<String, Error>
```

Compiles `source` and renders it in one call. The template is compiled on
every call, which costs tens of microseconds: right for a script, a test, or
a one-off message, wrong for a page served many times (use `Template` or
`Engine` there).

**Errors:** `ErrorKind::Compile`.

```rust
struct Note { text: String }
luminess::view!(Note as "note" { text: text });

fn main() -> Result<(), luminess::Error> {
    let html = luminess::render("<p>{{ note.text }}</p>", &Note { text: "a < b".into() })?;
    assert_eq!(html, "<p>a &lt; b</p>");
    Ok(())
}
```

## Errors: `Error`, `ErrorKind`, `Diagnostic`

Every call returns the same `Error`, so `?` works from loading a theme to
rendering a page, and it converts into `Box<dyn std::error::Error>`.

| `error.kind()` | When | Also |
|---|---|---|
| `ErrorKind::Compile` | one or more templates have mistakes; nothing was compiled | `diagnostics()`, `report()`, `to_html()` |
| `ErrorKind::Io` | a theme folder or file could not be read | `template()` is the path; `source()` is the `io::Error` |
| `ErrorKind::NotFound` | no template has the name asked for | `template()` is the name |
| `ErrorKind::TooLarge` | a render passed `Builder::max_output`; what it wrote is removed | `template()` is the name |

`ErrorKind` is `#[non_exhaustive]`: match it with a `_` arm.

`error.diagnostics()` lists each problem as a `Diagnostic`: `template()`,
`line()`, `column()`, `message()`, `source_line()`, and `included_from()`
(the template and line it was included from, when the problem shows only
there). `error.report()` writes them in the style of compiler errors, and
`error.to_html()` gives the same as an escaped `<pre>` block for a
development error page. Never show `to_html` to visitors of a live site: it
reveals template source.

```rust
use luminess::{Engine, ErrorKind};

struct Page { title: String }
luminess::view!(Page as "page" { title: text });

fn main() {
    let err = Engine::<Page>::builder()
        .template("home", "<h1>{{ page.titel }}</h1>")
        .template("nav", "{{ page.title | uper }}")
        .build()
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Compile);
    assert_eq!(err.diagnostics().len(), 2);
    let d = &err.diagnostics()[0];
    assert_eq!((d.template(), d.line(), d.column()), ("home", 1, 13));
    assert!(err.report().contains(" --> home:1:13"));
    assert!(err.to_html().starts_with("<pre class=\"luminess-errors\">"));
}
```

Handling each kind:

```rust
use luminess::{Engine, ErrorKind};

struct Page { title: String }
luminess::view!(Page as "page" { title: text });

fn status(err: &luminess::Error) -> u16 {
    match err.kind() {
        ErrorKind::NotFound => 404,
        ErrorKind::TooLarge | ErrorKind::Compile | ErrorKind::Io => 500,
        _ => 500,
    }
}

fn main() {
    let engine = Engine::<Page>::builder().build().unwrap();
    let err = engine.render("nope", &Page { title: String::new() }).unwrap_err();
    assert_eq!(status(&err), 404);
}
```

---

# Tier 2: more control

## Building a theme: `Engine::builder` and `Builder`

```text
Engine::<V>::builder() -> Builder<V>
```

For templates from strings, several folders, limits, and filters. Every
method takes and returns the builder; `build` compiles everything at once.

| Method | Does |
|---|---|
| `template(name, source)` | adds a template |
| `template_with_schema(name, source, schema)` | adds a template compiled against its own schema (a partial whose root is not the page, a second view shape) |
| `directory(dir)` | adds every `.html` file below `dir`; `Result<Builder, Error>` (`ErrorKind::Io`) |
| `directory_with_extension(dir, extension)` | the same for another extension (`"txt"` for mail) |
| `max_output(bytes)` | the most bytes one render may write; past it, `ErrorKind::TooLarge` (unlimited by default) |
| `max_template_size(bytes)` | the largest template source accepted; larger is a compile error (1 MiB by default) |
| `filter(name, params, returns, closure)` | adds an application filter (below) |
| `check()` | compiles and reports, keeping nothing and not consuming the builder (for CI and editors) |
| `build()` | compiles everything into an `Engine`; `ErrorKind::Compile` with every problem |

```rust
use luminess::Engine;

struct Page { title: String }
luminess::view!(Page as "page" { title: text });

fn main() -> Result<(), luminess::Error> {
    let engine = Engine::<Page>::builder()
        .template("base", "<main>{% block content %}{% endblock %}</main>")
        .template("home", "{% extends \"base\" %}{% block content %}{{ page.title }}{% endblock %}")
        .template("mail", "Hello {{ page.title }}")
        .max_output(64 * 1024)
        .build()?;
    assert_eq!(engine.render("home", &Page { title: "Hi".into() })?, "<main>Hi</main>");
    Ok(())
}
```

Checking a theme in CI without keeping it:

```rust
use luminess::Engine;

struct Page { title: String }
luminess::view!(Page as "page" { title: text });

fn main() {
    let theme = Engine::<Page>::builder().template("home", "{{ page.title }}").template("bad", "{% if %}");
    let err = theme.check().unwrap_err();
    assert_eq!(err.diagnostics()[0].template(), "bad");
}
```

A partial with its own root:

```rust
use luminess::{Engine, Schema};

struct Post { title: String }
struct Page { posts: Vec<Post> }
luminess::view!(Post as "post" { title: text });
luminess::view!(Page as "page" { posts: list(object(Post)) });

fn main() -> Result<(), luminess::Error> {
    let card = Schema::build("post").text("title").finish();
    let engine = Engine::<Page>::builder()
        .template("list", "{% for p in page.posts %}{% include \"card\" with p %}{% endfor %}")
        .template_with_schema("card", "<b>{{ post.title }}</b>", card)
        .build()?;
    let page = Page { posts: vec![Post { title: "One".into() }, Post { title: "Two".into() }] };
    assert_eq!(engine.render("list", &page)?, "<b>One</b><b>Two</b>");
    Ok(())
}
```

## Reusing a buffer: `render_into` and `Rendered`

```text
engine.render_into(name: &str, view: &V, out: &mut Vec<u8>) -> Result<Rendered, Error>
```

Appends the page to `out` instead of returning a new `String`, so a server
can keep one buffer per worker, and reports how the render went: `bytes()`,
`fallbacks()` (blocks that failed and fell back), and `failures()` (their
reasons, up to `Rendered::MAX_FAILURES`). Nothing is left in `out` from a
render that fails.

```rust
use luminess::Engine;

struct Page { title: String }
luminess::view!(Page as "page" { title: text });

fn main() -> Result<(), luminess::Error> {
    let engine = Engine::<Page>::builder().template("home", "<h1>{{ page.title }}</h1>").build()?;
    let mut buf = Vec::with_capacity(16 * 1024);
    for title in ["One", "Two", "Three"] {
        buf.clear();
        let report = engine.render_into("home", &Page { title: title.into() }, &mut buf)?;
        assert!(report.failures().is_empty());
        assert_eq!(report.bytes(), buf.len());
    }
    assert_eq!(buf, b"<h1>Three</h1>");
    Ok(())
}
```

## Swapping in a theme: `Engine::replace` and `Engine::contains`

```text
engine.replace(with: Engine<V>)
engine.contains(name: &str) -> bool
```

`replace` swaps in the templates of another engine for every clone of this
one; `reload_dir` is `replace(Engine::from_dir(dir)?)`. Building and swapping
are two steps, so a server can build every engine of a new theme (pages and
error pages), check that it has the templates the routes need with
`contains`, and swap only when all of it is good, or later, when a
configuration change commits.

```rust
use luminess::Engine;

struct Page { title: String }
luminess::view!(Page as "page" { title: text });

fn next_theme(home: &str) -> Result<Engine<Page>, String> {
    let engine = Engine::<Page>::builder()
        .template("home", home)
        .build()
        .map_err(|e| e.report())?;
    if !engine.contains("home") {
        return Err("the theme has no home page".into());
    }
    Ok(engine)
}

fn main() -> Result<(), String> {
    let live = next_theme("<h1>{{ page.title }}</h1>")?;
    let clone = live.clone();

    live.replace(next_theme("<h2>{{ page.title }}</h2>")?);
    assert_eq!(clone.render("home", &Page { title: "Hi".into() }).unwrap(), "<h2>Hi</h2>");

    // A broken theme never gets as far as `replace`.
    assert!(next_theme("{{ page.nope }}").is_err());
    assert_eq!(live.render("home", &Page { title: "Hi".into() }).unwrap(), "<h2>Hi</h2>");
    Ok(())
}
```

## Your own filters: `Builder::filter`

```text
builder.filter(name: &str, params: &[Kind], returns: Kind, f: impl Fn(Value, &[Value]) -> Answer)
```

Adds `{{ value | name(args) }}`. `params` are the kinds of the arguments and
`returns` the kind of the result; every use is checked against them at
compile time. The closure answers a borrowed `Value` or built text
(`Answer::Text`), both with `.into()`.

**Panics:** when `name` is a built-in filter or was added before (a mistake
in the program, found the first time it starts).

```rust
use luminess::{Answer, Engine, Kind, Value};

struct Order { cents: i64 }
luminess::view!(Order as "order" { cents: int });

fn main() -> Result<(), luminess::Error> {
    let engine = Engine::<Order>::builder()
        .filter("money", &[Kind::Text], Kind::Text, |v, args| {
            let (Value::Int(c), [Value::Text(cur)]) = (v, args) else { return Value::Absent.into() };
            Answer::Text(format!("{}.{:02} {cur}", c / 100, c % 100))
        })
        .template("total", r#"{{ order.cents | money("EUR") }}"#)
        .build()?;
    assert_eq!(engine.render("total", &Order { cents: 1999 })?, "19.99 EUR");

    // A wrong argument is a compile error, like a built-in filter's.
    let err = Engine::<Order>::builder()
        .filter("money", &[Kind::Text], Kind::Text, |v, _| v.into())
        .template("total", "{{ order.cents | money(1) }}")
        .build()
        .unwrap_err();
    assert!(err.to_string().contains("should be text"));
    Ok(())
}
```

## web-net: `web::Theme` and `web::ErrorView`

With the `web-net` feature, an `Engine<V>` is a `web_net::Render` whose view
type is `V`: a page handler's value goes straight to the compiled templates.
Template names a web-net app declares are resolved when the app is built, so
a missing template stops it from starting, and they keep working across
reloads.

`web::Theme::new(pages).with_error_pages(errors)` adds error pages from an
`Engine<web::ErrorView>`: `error/<status>`, then `error`, then web-net's
built-in page. An error template reads `error.status`, `error.title`,
`error.message`, and `error.reference`. `theme.pages()` and
`theme.error_pages()` return the engines, for `reload_dir` or `replace`.

```rust,ignore
use luminess::{Engine, web::{ErrorView, Theme}};

let pages = Engine::<Page>::from_dir("themes/default")?;
let errors = Engine::<ErrorView>::from_dir("themes/default/errors")?;
let app = web_net::App::with_gate(gate).render(Theme::new(pages).with_error_pages(errors));
```

---

# Tier 3: the view layer

Most programs never need this layer: `view!` writes it. It is here for views
written by hand, schemas known only at startup, and blocks that render their
own markup.

## `View`

```text
trait View {
    fn field(&self, index: usize) -> Value<'_>;
    fn call<'s>(&'s self, index: usize, args: &[Value<'_>]) -> Answer<'s> { /* Absent */ }
    fn render(&self, out: &mut Output<'_>) -> Result<(), BlockError> { /* fails */ }
    fn fallback(&self, out: &mut Output<'_>) { /* nothing */ }
    fn text(&self, out: &mut String) -> Result<(), BlockError> { /* fails */ }
}
```

What a template reads. `field` is called with the index of a field the
schema declared: its position, counting from 0 in declaration order. The
compiler resolved every name to its index at startup, so a render never
compares names. An index the view does not know answers `Value::Absent`.

```rust
use luminess::{Engine, Schema, Value, View};

struct Post { title: String, views: u32 }

impl View for Post {
    fn field(&self, index: usize) -> Value<'_> {
        match index {
            0 => Value::Text(&self.title),
            1 => Value::Int(i64::from(self.views)),
            _ => Value::Absent,
        }
    }
}

fn main() -> Result<(), luminess::Error> {
    let schema = Schema::build("post").text("title").int("views").finish();
    let engine = Engine::<Post>::with_schema(schema).template("t", "{{ post.title }}: {{ post.views }}").build()?;
    assert_eq!(engine.render("t", &Post { title: "Hi".into(), views: 3 })?, "Hi: 3");
    Ok(())
}
```

## `Value`, `List`, and `ToValue`

`Value<'a>` is one value read from a view: `Absent`, `Bool`, `Int`, `Float`,
`Text` (escaped for its place), `Html` (trusted markup: written as is in HTML
text, escaped anywhere else), `Display` (anything formattable, escaped like
text), `View` (an object), and `List`. It is `Copy` and only borrows.

`List` is what a template loops over: `len()` and `get(index)`. It is
implemented for `Vec<T>` and arrays of any `T: ToValue`.

`ToValue` turns a Rust value into a `Value`: strings, numbers, bools,
`Option` (absent when `None`), `Vec`, arrays, boxes, and `Arc`. `view!`
implements it for your structs; implement it by hand for a type written by
hand that other views hold as a field.

```rust
use luminess::{List, ToValue, Value};

fn main() {
    let tags = vec!["a".to_string(), "b".to_string()];
    assert!(matches!(tags.to_value(), Value::List(_)));
    assert_eq!(tags.len(), 2);
    assert!(matches!(tags.get(1), Value::Text("b")));
    assert!(matches!(None::<String>.to_value(), Value::Absent));
}
```

## Methods: `View::call` and `Answer`

A schema can declare methods: values computed from arguments, such as a
product's `variations.options("size")`. The compiler checks the argument
count and kinds; `View::call` answers with an `Answer`: a borrowed `Value`,
or `Answer::Text` for text built during the call.

```rust
use luminess::{Answer, Engine, HasSchema, Kind, Schema, ToValue, Value, View};

struct Variations { sizes: Vec<String>, prices: Vec<i64> }

impl HasSchema for Variations {
    fn schema() -> Schema {
        Schema::build("variations")
            .method("cheapest", &[], Kind::Int)
            .method("options", &[Kind::Text], Kind::list(Kind::Text))
            .finish()
    }
}

impl View for Variations {
    fn field(&self, _: usize) -> Value<'_> {
        Value::Absent
    }

    fn call<'s>(&'s self, index: usize, args: &[Value<'_>]) -> Answer<'s> {
        match (index, args) {
            (0, []) => Value::Int(self.prices.iter().copied().min().unwrap_or(0)).into(),
            (1, [Value::Text("size")]) => Value::List(&self.sizes).into(),
            _ => Value::Absent.into(),
        }
    }
}

impl ToValue for Variations {
    fn to_value(&self) -> Value<'_> {
        Value::View(self)
    }
}

struct Product { variations: Variations }
luminess::view!(Product as "product" { variations: object(Variations) });

fn main() -> Result<(), luminess::Error> {
    let t = luminess::Template::<Product>::new(
        r#"from {{ product.variations.cheapest() }}: {% for s in product.variations.options("size") %}{{ s }} {% endfor %}"#,
    )?;
    let p = Product { variations: Variations { sizes: vec!["S".into(), "M".into()], prices: vec![900, 700] } };
    assert_eq!(t.render(&p)?, "from 700: S M ");
    Ok(())
}
```

## Blocks that render themselves: `Output` and `BlockError`

A schema marked `renders()` belongs to a value that writes its own markup
when a template outputs it directly (`{{ page.gallery }}`). `View::render`
writes through an `Output`: `text` (escaped), `markup` (trusted, as is),
`url` (scheme-checked), `int`, and `write!` (escaped). When it returns a
`BlockError`, what it wrote is removed and `View::fallback` is written
instead; the rest of the page still renders, and the failure is reported in
`Rendered::failures`. `View::text` gives a block's plain-text form for an
attribute, a URL, a script, or a style.

```rust
use luminess::{BlockError, Engine, HasSchema, Output, Schema, ToValue, Value, View};

struct Stock { level: Result<u32, String> }

impl HasSchema for Stock {
    fn schema() -> Schema {
        Schema::build("stock").renders().finish()
    }
}

impl View for Stock {
    fn field(&self, _: usize) -> Value<'_> {
        Value::Absent
    }

    fn render(&self, out: &mut Output<'_>) -> Result<(), BlockError> {
        let n = self.level.as_ref().map_err(|e| BlockError::new(e.clone()))?;
        out.markup("<b>");
        out.int(i64::from(*n));
        out.markup(" left</b>");
        Ok(())
    }

    fn fallback(&self, out: &mut Output<'_>) {
        out.text("Stock unknown");
    }
}

impl ToValue for Stock {
    fn to_value(&self) -> Value<'_> {
        Value::View(self)
    }
}

struct Page { stock: Stock }
luminess::view!(Page as "page" { stock: object(Stock) });

fn main() -> Result<(), luminess::Error> {
    let engine = Engine::<Page>::builder().template("p", "<p>{{ page.stock }}</p>").build()?;
    assert_eq!(engine.render("p", &Page { stock: Stock { level: Ok(3) } })?, "<p><b>3 left</b></p>");

    let mut out = Vec::new();
    let report = engine.render_into("p", &Page { stock: Stock { level: Err("service down".into()) } }, &mut out)?;
    assert_eq!(out, b"<p>Stock unknown</p>");
    assert_eq!(report.failures()[0].reason(), "service down");
    Ok(())
}
```

## Schemas: `Schema`, `SchemaBuilder`, `Kind`, `HasSchema`

A `Schema` names the fields and methods a view answers, with the `Kind` of
each, in index order. `Schema::build(name)` starts a `SchemaBuilder`:
`text`, `html`, `int`, `float`, `bool`, `object(name, schema)`,
`list(name, kind)`, `field(name, kind)`, `method(name, params, returns)`,
`renders()`, then `finish()`. Declaring a name twice panics (a mistake in
the program).

`Kind` is `Bool`, `Int`, `Float`, `Text`, `Html`, `Object(Schema)`, or
`List(kind)`; `Kind::object_of::<T>()` and `Kind::list_of::<T>()` take the
schema from a type, built the first time it is needed, so a type can refer to
itself (a comment with a list of replies).

`HasSchema` is a Rust type whose schema is fixed. `view!` implements it, and
`Engine::<V>::from_dir`, `Engine::<V>::builder`, and `Template::<V>::new`
need it to know what to check templates against.

```rust
use luminess::{Kind, Schema};

fn main() {
    let post = Schema::build("post").text("title").text("url").finish();
    let page = Schema::build("page").text("title").html("body").list("posts", Kind::Object(post)).finish();
    assert_eq!(page.name(), "page");
    assert_eq!(page.len(), 3);
    assert_eq!(page.field("body").map(|(index, _)| index), Some(1));
    assert_eq!(page.names().collect::<Vec<_>>(), ["title", "body", "posts"]);
}
```

## Schemas built at startup: `Engine::with_schema`

```text
Engine::<V>::with_schema(schema: Schema) -> Builder<V>
```

A builder whose templates compile against `schema` rather than `V`'s own:
for a view type whose fields are decided at startup, by configuration or a
plugin.

```rust
use luminess::{Engine, Schema, Value, View};

struct Doc(Vec<String>);

impl View for Doc {
    fn field(&self, index: usize) -> Value<'_> {
        self.0.get(index).map_or(Value::Absent, |s| Value::Text(s))
    }
}

fn main() -> Result<(), luminess::Error> {
    // Fields read from configuration at startup.
    let fields = ["summary", "sku"];
    let schema = fields.iter().fold(Schema::build("doc"), |b, f| b.text(f)).finish();
    let engine = Engine::<Doc>::with_schema(schema).template("card", "{{ doc.summary }} ({{ doc.sku }})").build()?;
    assert_eq!(engine.render("card", &Doc(vec!["Shoes".into(), "S-1".into()]))?, "Shoes (S-1)");
    Ok(())
}
```

## Escapers: `luminess::escape`

The escapers the engine picks from, for code that builds markup by hand.
Each appends to a `Vec<u8>` and never fails. `docs/SECURITY.md` says what
each guarantees.

| Function | For |
|---|---|
| `html(s, out)` | HTML text and quoted attribute values |
| `url(s, out)`, `url_allowed(s)` | a URL attribute (normalized; check the scheme first) |
| `url_component(s, out)` | one query value or path segment |
| `js_string(s, out)`, `js_quoted(s, out)` | JavaScript string content, or a whole quoted literal |
| `css(s, out)`, `css_value_allowed(s)` | CSS string content, or a check for a value written as is |
| `BLOCKED_URL`, `BLOCKED_CSS` | what replaces a refused URL or CSS value |

```rust
use luminess::escape;

fn main() {
    let mut out = Vec::new();
    escape::html("Tom & <Jerry>", &mut out);
    assert_eq!(out, b"Tom &amp; &lt;Jerry&gt;");

    assert!(escape::url_allowed("https://example.com"));
    assert!(!escape::url_allowed("javascript:alert(1)"));

    let mut q = Vec::new();
    escape::url_component("a&b c", &mut q);
    assert_eq!(q, b"a%26b%20c");
}
```

---

# Why these names

| Name | Why |
|---|---|
| `Engine::<V>` | the type is named because it is what every template is checked against; that check is the point of the crate |
| `Template` | one compiled template, as in any engine; an `Engine` is a named set of them |
| `render` / `render_into` | `render` returns the page, the obvious call; `render_into` writes into a buffer you keep |
| `Error`, `ErrorKind` | one error for every call, so `?` works everywhere |
| `HasSchema` | read as "this type has a schema": what an engine needs to compile against it without being handed one |
| `Answer` | what a method or filter answers: a borrowed value or text built for the call |
| `BlockError` | the error a block (a value that renders itself) returns when it cannot render |
| `index: usize` in `View` | a field is asked for by its position in the schema, a plain number |
