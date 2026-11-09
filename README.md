# Luminess

A template engine for Rust that compiles once, at startup, against typed views.

[![crates.io](https://img.shields.io/crates/v/luminess.svg)](https://crates.io/crates/luminess)
[![docs.rs](https://docs.rs/luminess/badge.svg)](https://docs.rs/luminess)
[![CI](https://github.com/jamesgober/Luminess/actions/workflows/ci.yml/badge.svg)](https://github.com/jamesgober/Luminess/actions/workflows/ci.yml)

- **Mistakes fail at startup.** Templates compile against a schema of your
  types. `{{ page.titel }}` is a compile error with the template, line,
  column, and "did you mean `title`?", not an empty string in production.
- **Escaped for where it goes.** The compiler follows the HTML around each
  `{{ }}` and picks the escaper: HTML text, attribute value, URL (with a
  scheme check, so `javascript:` never reaches an `href`), JavaScript, or
  CSS. Places where no escaper is safe, such as an unquoted attribute, an
  HTML comment, or a JavaScript regular expression, are refused at compile
  time.
- **No data conversion.** No JSON, no maps, no serde. A render reads your
  values through a small trait by field ids the compiler resolved, and writes
  straight into your `Vec<u8>`.
- **Blocks render themselves, and fail alone.** A value can write its own
  markup (a gallery, a price table). If it fails, its output is removed, its
  fallback is written, and the rest of the page renders.
- **Live reload.** Swap in a new theme while serving. A theme with errors is
  refused and the running one stays.
- **Fast.** A typical 10 KiB page renders in about 9 µs, 3 to 5 times
  faster than Tera and MiniJinja.
- **Zero dependencies.**

## Install

```toml
[dependencies]
luminess = "0.7"
```

MSRV: Rust 1.85.

## Quick start

```rust
use luminess::Engine;

struct Post { title: String, url: String }
struct Page { title: String, posts: Vec<Post> }

// Declare what templates may read. Field names are the struct's own.
luminess::view!(Post as "post" { title: text, url: text });
luminess::view!(Page as "page" { title: text, posts: list(object(Post)) });

let engine = Engine::<Page>::builder()
    .template("home", r#"<h1>{{ page.title }}</h1>
<ul>
{%- for post in page.posts %}
  <li><a href="{{ post.url }}">{{ post.title }}</a></li>
{%- endfor %}
</ul>"#)
    .build()?; // every template error, all at once

let page = Page {
    title: "News & notes".into(),
    posts: vec![Post { title: "First".into(), url: "/first".into() }],
};

let mut out = Vec::new();
engine.render("home", &page, &mut out)?;
```

## Templates

The syntax follows Jinja and Twig.

```text
{{ page.title }}                       output, escaped for where it sits
{{ page.body }}                        an html field is trusted markup, written as is in HTML text
{{ page.note | raw }}                  explicit unescaped output
{{ page.subtitle | default("None") }}  filters
{{ page.variations.options("size") }}  methods with arguments, checked at compile time
{% set name = page.author.name | upper %}
{% if page.featured %}...{% elif page.draft %}...{% else %}...{% endif %}
{% for post in page.posts %}{{ loop.index }}. {{ post.title }}{% else %}No posts.{% endfor %}
{% include "partials/nav" %}
{% include "card" with post %}         the partial sees `post` as its own root
{% extends "base" %}  {% block content %}...{{ super() }}...{% endblock %}
{% try %}{{ page.stock }}{% fallback %}Stock unknown{% endtry %}
{# a comment #}   {% raw %}{{ not parsed }}{% endraw %}   {{- trims whitespace -}}
```

Conditions take `not`, `and`, `or`, `==`, `!=`, `<`, `<=`, `>`, `>=`,
parentheses, and literals (strings, integers, decimals, `true`, `false`). In a
loop, `loop.index`, `loop.index0`, `loop.revindex`, `loop.first`, `loop.last`,
and `loop.length` are available.

Filters: `default(v)`, `upper`, `lower`, `trim`, `length`, `truncate(n)`,
`join(sep)`, `first`, `last`, `replace(from, to)`, `date(format)` (a Unix time
in seconds, UTC; `%Y %m %d %H %M %S %b %B %%`), `number` (groups digits:
`1,234,567`), `raw`, and `fallback(text)` for blocks. Add your own with
`Builder::filter`:

```rust
use luminess::{Computed, Engine, Kind, Value};

let engine = Engine::<Order>::builder()
    .filter("money", &[Kind::Text], Kind::Text, |v, args| {
        let (Value::Int(cents), [Value::Text(cur)]) = (v, args) else { return Value::Absent.into() };
        Computed::Text(format!("{}.{:02} {cur}", cents / 100, cents % 100))
    })
    .template("total", r#"{{ order.cents | money("EUR") }}"#)
    .build()?;
```

The compiler checks every use of a filter against the argument kinds you
declare.

False in a condition: `false`, an absent value, zero, the empty string, and
the empty list.

## Layouts and partials

A base template marks the places a page may replace with `block`:

```text
{# base #}
<html><head><title>{% block title %}My site{% endblock %}</title></head>
<body>{% block content %}{% endblock %}</body></html>

{# post #}
{% extends "base" %}
{% block title %}{{ page.title }} | {{ super() }}{% endblock %}
{% block content %}<article>{{ page.body }}</article>{% endblock %}
```

A template that extends another holds only blocks, and every block it
defines must exist further up the chain, so a misspelled block name is a
compile error. Chains can be several levels deep; `super()` writes the next
version up.

`{% include "name" %}` inserts a partial that sees the same values as the
place it is included from. `{% include "card" with post %}` gives the partial
one value as its root instead, named by the partial's own schema (add it
with `template_with`). The partial is checked against the value it is given.

Load a whole theme from a folder; names are paths without the extension:

```rust
let engine = Engine::<Page>::builder().directory("themes/default", "html")?.build()?;
// themes/default/partials/nav.html is "partials/nav"
```

One set can serve several view shapes: `template_with(name, source, schema)`
compiles a template against its own schema, so a product page and a blog
post can be rendered from the same engine as long as they share the Rust
view type.

## Escaping

| Where the `{{ }}` sits | What is written |
|---|---|
| HTML text | `& < > " '` as entities; `html` fields as is |
| a quoted attribute value, `<title>`, `<textarea>` | `& < > " '` as entities, `html` fields too |
| the start of a URL attribute (`href`, `src`, `action`, ...) | only `http`, `https`, `mailto`, `tel`, and relative URLs pass; anything else becomes `#blocked`; then percent-encoded |
| later in a URL, before `?` | percent-encoded where a URL does not allow the byte |
| after `?` or `#` in a URL | every reserved byte percent-encoded |
| JavaScript code in `<script>` or an `on...` attribute | a complete literal: text as a double-quoted string, numbers, `true`/`false`, absent as `null` |
| a JavaScript string or template literal | its content, with quotes, backslashes, `<`, `>`, `&`, `$`, braces, and line breaks escaped |
| a CSS value in `<style>` or a `style` attribute | the value as is when it holds only letters, digits, spaces, and `# % . , - _` (colors, lengths, names); `blocked` otherwise |
| a CSS string | every character other than letters and digits as a CSS hex escape |

```text
<script>var user = {{ page.user }};</script>      -> var user = "Ann O\u0027Neil";
<button onclick="open({{ page.id }})">            -> open(42)
<style>h1 { color: {{ page.color }}; }</style>    -> color: #ff0000;
```

Refused at compile time: output in an unquoted attribute value, a tag name, an
attribute name, an HTML comment, `srcdoc`, a JavaScript comment or regular
expression, a CSS comment, right after `$` in a template literal, and in an
`on...` or `style` attribute whose text uses character references (`&quot;`).
A template that ends inside a tag, an attribute, a comment, a script, or a
style is an error too. `| raw` is allowed in HTML text only, so
`grep '| raw'` finds every unescaped value in a theme.

The escapers are public in `luminess::escape` for code that builds markup by
hand.

## Views

`view!` covers plain structs. Implement `View` by hand for anything else:
the schema gives every field an id (its position), and `field` answers by id.

```rust
use luminess::{Engine, FieldId, Schema, Value, View};

struct Product { name: String, price_cents: i64 }

impl View for Product {
    fn field(&self, id: FieldId) -> Value<'_> {
        match id.index() {
            0 => Value::Text(&self.name),
            1 => Value::Int(self.price_cents),
            _ => Value::Absent,
        }
    }
}

// A schema can be built at startup, from configuration or a plugin.
let schema = Schema::build("product").text("name").int("price_cents").finish();
let engine = Engine::<Product>::builder_with(schema)
    .template("card", "<b>{{ product.name }}</b> {{ product.price_cents }}")
    .build()?;
```

Values are `Text`, `Html` (trusted markup), `Int`, `Float`, `Bool`, `Display`
(anything formattable, escaped like text), `View` (an object), `List`, and
`Absent`.

A schema can declare methods: values computed from arguments, such as a
product's `variations.options("size")`. The compiler checks the argument
count and kinds; `View::call` answers, returning a borrowed value or text
built for the call (`Computed`):

```rust
use luminess::{Computed, FieldId, Kind, Schema, Value, View};

let schema = Schema::build("variations")
    .method("cheapest", &[], Kind::Int)
    .method("options", &[Kind::Text], Kind::list(Kind::Text))
    .finish();

impl View for Variations {
    fn field(&self, _: FieldId) -> Value<'_> { Value::Absent }

    fn call<'s>(&'s self, id: FieldId, args: &[Value<'_>]) -> Computed<'s> {
        match (id.index(), args) {
            (0, []) => Value::Int(self.cheapest_cents()).into(),
            (1, [Value::Text(axis)]) => Value::List(self.options(axis)).into(),
            _ => Value::Absent.into(),
        }
    }
}
```

## Blocks and fallbacks

A schema marked `renders()` belongs to a value that writes its own markup
when a template outputs it directly:

```rust
use luminess::{Failed, FieldId, Output, Value, View};

struct Stock { level: Result<u32, String> }

impl View for Stock {
    fn field(&self, _: FieldId) -> Value<'_> { Value::Absent }

    fn render(&self, out: &mut Output<'_>) -> Result<(), Failed> {
        let n = self.level.as_ref().map_err(|e| Failed::new(e.clone()))?;
        out.markup("<span class=\"stock\">");
        out.int(i64::from(*n));
        out.markup(" left</span>");
        Ok(())
    }

    fn fallback(&self, out: &mut Output<'_>) {
        out.text("Stock unavailable");
    }
}
```

When `render` fails, what it wrote is removed and `fallback` is written.
In a template, `{{ page.stock | fallback("Unavailable") }}` writes that text
instead, and `{% try %}...{% fallback %}...{% endtry %}` replaces a whole
region. Either way the render succeeds; `Rendered::fallbacks` counts the
failures and `Rendered::failures` keeps their reasons (up to 16) for the log.

A block can also sit where markup cannot go, such as an attribute, a URL, or a
script, through its text form: implement `View::text`, and the engine
escapes the text for the place.

## Live reload

```rust
// A theme changed on disk: compile the new set and swap it in.
match engine.reload(Engine::<Page>::builder().template("home", new_source)) {
    Ok(()) => {}                       // renders from now on use the new set
    Err(errors) => eprintln!("{errors}"), // the running set stays
}
```

`Engine` is cheap to clone and safe to share across threads. A render in
flight finishes on the set it started with.

## Errors

```text
3 template errors
  home:1:13: `page` (page) has no field `titel`, did you mean `title`?
  home:4:20: output in an unquoted attribute value; put the value in double quotes
  nav:2:4: unknown filter `uper`, did you mean `upper`?; the filters are default, upper, lower, trim, length, raw
```

`CompileError::diagnostics` gives each one with its template, line, and
column.

## Performance

`cargo bench --bench render` renders a typical page: a head, a navigation
include with 6 links, a 2 KiB trusted HTML body, 20 related posts with tags,
and a footer, about 10.8 KiB of output and 150 values.

| Case | Time |
|---|---|
| typical page | about 9 µs |
| one value | about 60 ns |
| compiling the page and its include | about 40 µs |
| escaping text for HTML | about 4 GB/s with nothing to escape |

`benches/compare` renders the same page with other Rust engines
(`cargo run --release` in that folder):

| Engine | Typical page |
|---|---|
| Askama 0.16 | about 4 µs |
| **Luminess** | about 9 µs |
| Tera 2 | about 28 µs |
| MiniJinja 3 | about 45 µs |

Askama turns templates into Rust code when your program is built, so it is
faster still, but it cannot reload a theme or compile against schemas known
only at startup. Tera and MiniJinja convert the data through serde on every
render; Luminess reads your values in place. Luminess also does more per
value than the others here: URLs are checked and script values become
JavaScript literals.

A render takes no lock and makes no shared atomic write: each thread keeps
the engine's current set and checks one counter, so renders on many threads
do not contend.

Measured on a shared Windows 11 desktop machine under load from other work;
the fastest of seven runs.

## License

Apache-2.0. See [LICENSE](LICENSE).
