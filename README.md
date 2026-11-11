<h1 align="center">
    <img width="99" alt="Rust logo" src="https://raw.githubusercontent.com/jamesgober/rust-collection/72baabd71f00e14aa9184efcb16fa3deddda3a0a/assets/rust-logo.svg">
    <br>
    <b>Luminess</b>
    <br>
    <sub>
        <sup>TEMPLATES CHECKED AT STARTUP, ESCAPED FOR WHERE THEY GO</sup>
    </sub>
</h1>

<p align="center">
    <a href="https://crates.io/crates/luminess"><img src="https://img.shields.io/crates/v/luminess.svg" alt="Crates.io"></a>
    <a href="https://crates.io/crates/luminess" alt="Download"><img alt="Crates.io Downloads" src="https://img.shields.io/crates/d/luminess?color=%230099ff"></a>
    <a href="https://docs.rs/luminess"><img src="https://docs.rs/luminess/badge.svg" alt="Documentation"></a>
    <a href="https://github.com/jamesgober/Luminess/actions"><img alt="GitHub CI" src="https://github.com/jamesgober/Luminess/actions/workflows/ci.yml/badge.svg"></a>
    <img src="https://img.shields.io/badge/MSRV-1.85-orange.svg" alt="MSRV 1.85">
    <a href="#license"><img src="https://img.shields.io/badge/license-Apache--2.0-blue.svg" alt="License"></a>
</p>

<p align="center">
    <b>A template engine for Rust that compiles your theme once, against your own types.</b>
    <br>
    <i>A misspelled field fails at startup, not as a blank page; every value is escaped for the place it lands.</i>
</p>

<br>

<p>
    <strong>Luminess</strong> renders HTML from templates in a Jinja-like syntax. It compiles a whole theme when your program starts, checks every name in it against the Rust types you render, and then renders pages in a few microseconds by reading your values in place and writing straight into a buffer.
</p>

<p>
    Where most template engines take a map of loosely typed values and escape everything as HTML text, <strong>Luminess</strong> knows the type of every field and the HTML context of every output. An unknown field, filter, template, or block is an error with its line and column before the first request, and each value gets the escaping its place needs: HTML text, attribute, URL (with a scheme check), JavaScript, or CSS.
</p>

<p>
    It reloads a theme while serving, lets a block (a gallery, a stock level) render itself and fall back alone when it fails, and has no dependencies.
</p>

---

## Features

### **Checked at startup**
- **Typed views** - templates compile against your types; a misspelled field or a list used as text is an error with "did you mean"
- **Every error at once** - all mistakes in all templates, with template, line, column, and the source line
- **One error type** - loading, compiling, and rendering return the same `Error`, so `?` just works

### **Escaped for where it goes**
- **Contextual escaping** - HTML text, attributes, URLs, JavaScript, and CSS each get their own escaper, chosen at compile time
- **Unsafe places refused** - output in unquoted attributes, comments, tag names, or regular expressions is a compile error
- **Explicit raw output** - `| raw` is the only way to skip escaping, so it is easy to find

### **Built for serving**
- **Fast** - about 4 µs for a typical 10 KiB page; values read in place, no conversion
- **Live reload** - swap a theme in while serving; a theme with errors is refused and the running one stays
- **Blocks that fail alone** - a block that cannot render is replaced by its fallback; the page still goes out
- **Zero dependencies, no unsafe code**

---

## Installation

```toml
[dependencies]
luminess = "0.10"

# With the web-net integration:
luminess = { version = "0.10", features = ["web-net"] }
```

MSRV: Rust 1.85.

---

## Quick Start

Describe what templates may read, load the theme, render:

```rust
use luminess::Engine;

struct Page { title: String, body: String }
luminess::view!(Page as "page" { title: text, body: html });

let engine = Engine::<Page>::from_dir("themes/default")?;   // every .html file below it
let html = engine.render("home", &Page { title: "News".into(), body: "<p>Hi</p>".into() })?;
```

One template, compiled once:

```rust
let card = luminess::Template::<Page>::new("<h1>{{ page.title }}</h1>")?;
let html = card.render(&page)?;
```

One render that compiles on the spot, for scripts and tests:

```rust
let html = luminess::render("<b>{{ page.title }}</b>", &page)?;
```

### **Reload a theme while serving**

```rust
engine.reload_dir("themes/default")?;   // on error the running theme stays
```

### **Reuse one buffer per request**

```rust
let mut buf = Vec::with_capacity(16 * 1024);
buf.clear();
let report = engine.render_into("home", &page, &mut buf)?;
for failure in report.failures() {
    eprintln!("a block fell back: {failure}");
}
```

### **More templates, limits, and filters**

```rust
let engine = Engine::<Page>::builder()
    .directory("themes/default")?
    .template("email", "Hello {{ page.title }}")
    .max_output(1 << 20)
    .build()?;
```

The type is named in `Engine::<Page>` on purpose: it is what the templates are checked against.

---

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
use luminess::{Answer, Engine, Kind, Value};

let engine = Engine::<Order>::builder()
    .filter("money", &[Kind::Text], Kind::Text, |v, args| {
        let (Value::Int(cents), [Value::Text(cur)]) = (v, args) else { return Value::Absent.into() };
        Answer::Text(format!("{}.{:02} {cur}", cents / 100, cents % 100))
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
with `template_with_schema`). The partial is checked against the value it is given.

Load a whole theme from a folder; names are paths without the extension:

```rust
let engine = Engine::<Page>::from_dir("themes/default")?;
// themes/default/partials/nav.html is "partials/nav"
```

`template_with_schema(name, source, schema)` compiles a template against its own
schema, for a partial included `with` a value of that schema. Rendered by
name, a template is always given the engine's view type, so one added this
way can be rendered by name only when its schema reads that type the same
way; otherwise it is only for `include ... with`.

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
| a CSS value in `<style>` or a `style` attribute | the value as is when it holds only letters, digits, spaces, `# % . , - _ + * /`, and the functions `rgb rgba hsl hsla calc min max clamp var` (colors, lengths, names, sizes); `blocked` otherwise |
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
use luminess::{Engine, Schema, Value, View};

struct Product { name: String, price_cents: i64 }

impl View for Product {
    fn field(&self, index: usize) -> Value<'_> {
        match index {
            0 => Value::Text(&self.name),
            1 => Value::Int(self.price_cents),
            _ => Value::Absent,
        }
    }
}

// A schema can be built at startup, from configuration or a plugin.
let schema = Schema::build("product").text("name").int("price_cents").finish();
let engine = Engine::<Product>::with_schema(schema)
    .template("card", "<b>{{ product.name }}</b> {{ product.price_cents }}")
    .build()?;
```

Values are `Text`, `Html` (trusted markup), `Int`, `Float`, `Bool`, `Display`
(anything formattable, escaped like text), `View` (an object), `List`, and
`Absent`.

A schema can declare methods: values computed from arguments, such as a
product's `variations.options("size")`. The compiler checks the argument
count and kinds; `View::call` answers, returning a borrowed value or text
built for the call (`Answer`):

```rust
use luminess::{Answer, Kind, Schema, Value, View};

let schema = Schema::build("variations")
    .method("cheapest", &[], Kind::Int)
    .method("options", &[Kind::Text], Kind::list(Kind::Text))
    .finish();

impl View for Variations {
    fn field(&self, _: usize) -> Value<'_> { Value::Absent }

    fn call<'s>(&'s self, index: usize, args: &[Value<'_>]) -> Answer<'s> {
        match (index, args) {
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
use luminess::{BlockError, Output, Value, View};

struct Stock { level: Result<u32, String> }

impl View for Stock {
    fn field(&self, _: usize) -> Value<'_> { Value::Absent }

    fn render(&self, out: &mut Output<'_>) -> Result<(), BlockError> {
        let n = self.level.as_ref().map_err(|e| BlockError::new(e.clone()))?;
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
// A theme changed on disk: compile it again and swap it in.
if let Err(e) = engine.reload_dir("themes/default") {
    eprintln!("{}", e.report());   // the running theme stays
}
```

For a theme built another way, build the new engine and swap it in with
`engine.replace(new_engine)`: building and swapping are two steps, so a
server can check a new theme fully before any of it goes live.
`Engine` is cheap to clone and safe to share across threads; a reload
reaches every clone, and a render in flight finishes on the theme it
started with.

## With web-net

The `web-net` feature makes an `Engine` a [web-net](https://crates.io/crates/web-net)
template engine, reading your page views directly:

```toml
luminess = { version = "0.10", features = ["web-net"] }  # web-net 0.8 or 0.9
```

```rust
use luminess::{Engine, web::{ErrorView, Theme}};

let pages = Engine::<Page>::builder().directory("themes/default")?.build()?;
let errors = Engine::<ErrorView>::builder().directory("themes/default/errors", "html")?.build()?;
let app = web_net::App::with_gate(gate).render(Theme::new(pages).with_error_pages(errors));
```

Route templates are resolved when the app is built, so a missing template
stops it from starting; the ids stay valid across reloads. Error pages
come from `error/<status>`, then `error`, then web-net's built-in page; they
read `error.status`, `error.title`, `error.message`, and `error.reference`.
Views must be `'static` (owned data or `Arc`). The default build of Luminess
depends on nothing.

## Errors

```text
3 template errors
  home:1:13: `page` (page) has no field `titel`, did you mean `title`?
  home:4:20: output in an unquoted attribute value; put the value in double quotes
  nav:2:4: unknown filter `uper`, did you mean `upper`?; the filters are default, upper, lower, trim, length, truncate, join, first, last, replace, date, number, raw
```

`Error::diagnostics` gives each one with its template, line,
column, and source line. `Error::report` writes them in the style of
compiler errors, with a caret under the column:

```text
error: `page` (page) has no field `titel`, did you mean `title`?
 --> home:1:13
  |
1 | <h1>{{ page.titel }}</h1>
  |             ^
```

`Error::to_html` gives the same as an escaped `<pre>` block for a
development error page (never for visitors: it shows template source).
`Builder::check` compiles a theme and keeps nothing, for an editor or CI.

## Limits

| Limit | Default | Set with |
|---|---|---|
| bytes one render may write | unlimited | `Builder::max_output` |
| bytes in one template source | 1 MiB | `Builder::max_template_size` |
| nesting of statements and expressions | 64 | fixed |
| nesting of includes, length of `extends` chains | 8 each | fixed |
| compiled size of one template with its includes | about 1 million nodes | fixed |

A render that passes `max_output` stops, removes what it wrote, and fails with
`ErrorKind::TooLarge`.

## Performance

`cargo bench --bench render` renders a typical page: a head, a navigation
include with 6 links, a 2 KiB trusted HTML body, 20 related posts with tags,
a script block with three values, and a footer, about 10.9 KiB of output and 150 values.

| Case | Time |
|---|---|
| typical page | about 4 µs |
| one value, whole render | about 40 ns |
| compiling the page and its include | about 28 µs |
| escaping text for HTML | about 12 GB/s with nothing to escape |

`benches/compare` renders the same page with other Rust engines
(`cargo run --release` in that folder):

| Engine | Typical page |
|---|---|
| Askama 0.16 | about 2 µs |
| **Luminess** | about 3.6 µs |
| Tera 2 | about 16 µs |
| MiniJinja 3 | about 26 µs |

Askama turns templates into Rust code when your program is built, so it is
faster still, but it cannot reload a theme or compile against schemas known
only at startup. Tera and MiniJinja convert the data through serde on every
render; Luminess reads your values in place. Luminess also does more per
value than the others here: URLs are checked and script values become
JavaScript literals.

A render takes no lock and makes no shared atomic write: each thread keeps
the engine's current set and checks one counter, so renders on many threads
do not contend.

Measured on a shared Windows 11 desktop while it was quiet, the fastest of
seven runs; `docs/PERFORMANCE.md` has the ranges and the 0.9.2 comparison.

## Status

**Current version:** `0.10.0`, the last planned 0.x reshaping of the API. `1.0.0` waits for its own decision; the checklist is in `dev/ROADMAP.md`. Until then the API may still change, with a migration note in `CHANGELOG.md` for every change.

## Documentation

- [Template syntax](docs/SYNTAX.md)
- [API reference](docs/API.md): every public item, with examples, simplest first
- [Security model](docs/SECURITY.md): what each escaper guarantees, and the known limits
- [Performance](docs/PERFORMANCE.md): numbers, method, and the comparison with other engines
- API documentation on [docs.rs](https://docs.rs/luminess)

### **Version compatibility**
- **Rust**: 1.85 or newer (edition 2024)
- **Dependencies**: none by default; the `web-net` feature adds web-net 0.8 or 0.9

### **Development setup**
```bash
git clone https://github.com/jamesgober/Luminess.git
cd Luminess
cargo test --all-features
cargo bench --bench render
```

<hr>

<div id="license">
    <h2>License</h2>
    <p><b>Luminess</b> is licensed under the <b>Apache License, Version 2.0</b>: see <a href="./LICENSE" title="Apache-2.0 License">LICENSE</a> or <a href="http://www.apache.org/licenses/LICENSE-2.0" title="Apache-2.0 License" target="_blank">apache.org/licenses/LICENSE-2.0</a>.</p>
    <p>Unless required by applicable law or agreed to in writing, software distributed under the License is distributed on an <b>"AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND</b>, either express or implied.</p>
</div>

<div align="center">
  <h2></h2>
  <sup>COPYRIGHT <small>&copy;</small> 2026 <strong>JAMES GOBER.</strong></sup>
</div>
