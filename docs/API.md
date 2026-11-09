# API overview

Everything public is at the crate root, plus the `escape` module and the
`view!` macro.

## Compile

| Item | Purpose |
|---|---|
| `Engine::<V>::builder()` | a `Builder` for views of type `V` (needs `V: Shaped`, which `view!` implements) |
| `Engine::<V>::builder_with(schema)` | the same with a schema built at startup |
| `Builder::template(name, source)` | adds a template |
| `Builder::template_with(name, source, schema)` | adds a template with its own schema (a partial's root, a second view shape) |
| `Builder::directory(dir, extension)` | adds every matching file below `dir`, named by relative path without the extension |
| `Builder::filter(name, params, returns, closure)` | adds an application filter, checked like built-ins |
| `Builder::max_output(bytes)`, `Builder::max_template_size(bytes)` | limits |
| `Builder::check()` | compiles and reports, keeping nothing |
| `Builder::compile()` | the compiled `Templates` |
| `Builder::build()` | a new `Engine` serving them |

Every error of every template comes back at once in a `CompileError`:
`diagnostics()` (each a `Diagnostic` with `template`, `line`, `column`,
`message`, `source_line`, `included_from`), `report()` (compiler-style text),
and `to_html()` (for development error pages).

## Render

| Item | Purpose |
|---|---|
| `Engine::render(name, &view, &mut Vec<u8>)` | appends the page; returns `Rendered` |
| `Engine::render_to_string(name, &view)` | the same into a new `String` |
| `Engine::reload(builder)` | compiles and swaps in a new set; on errors the running set stays |
| `Engine::swap(templates)`, `Engine::templates()` | swap in a compiled set; take the current one |
| `Templates::render`, `render_to_string`, `contains`, `names`, `len` | a fixed snapshot |
| `Rendered::bytes`, `fallbacks`, `failures`, `first_failure`, `is_complete` | what one render reports |
| `RenderError::kind` | `NotFound` (no such template) or `TooLarge` (`max_output` passed) |

`Engine` is cheap to clone and `Send + Sync`; renders on many threads share
nothing mutable.

## Views

| Item | Purpose |
|---|---|
| `View::field(id)` | the value of a field, by the id the compiler resolved |
| `View::call(id, args)` | a method's value (`Computed`: a borrowed `Value` or built text) |
| `View::render(out)`, `View::fallback(out)` | a block's own markup, and what replaces it on failure |
| `View::text(out)` | a block's text form, for attributes, URLs, scripts, and styles |
| `Value` | `Absent`, `Bool`, `Int`, `Float`, `Text`, `Html`, `Display`, `View`, `List` |
| `List` | `len` and `get` for loops |
| `ToValue` | Rust values to `Value` (strings, numbers, bools, `Option`, `Vec`, arrays, boxes) |
| `Schema::build(name)` | a `SchemaBuilder`: `text`, `html`, `int`, `float`, `bool`, `object`, `list`, `field`, `method`, `renders`, `finish` |
| `Kind` | a field's kind; `Kind::object_of::<T>()`, `Kind::list_of::<T>()`, `Kind::list(item)` |
| `SchemaRef` | a schema built already, or a function that builds it (for types that refer to themselves) |
| `Shaped` | a Rust type with a fixed schema |
| `view!(Type as "name" { field: kind, ... })` | implements `View`, `Shaped`, and `ToValue` for a struct |
| `Output` | where a block writes: `text` (escaped), `markup` (trusted), `url`, `int`, and `fmt::Write` |
| `Failed` | why a block could not render |

## web-net (feature `web-net`)

| Item | Purpose |
|---|---|
| `impl web_net::Render for Engine<V>` | the engine as web-net's template engine; `type View<'a> = V` |
| `web::Theme::new(pages).with_error_pages(errors)` | pages plus error pages |
| `web::ErrorView` | what an error page reads: `status`, `title`, `message`, `reference` |

## Escapers

`escape::html`, `escape::url`, `escape::url_component`, `escape::url_allowed`,
`escape::js_string`, `escape::js_quoted`, `escape::css`,
`escape::css_value_allowed`, and the replacement constants `BLOCKED_URL` and
`BLOCKED_CSS`. Each appends to a `Vec<u8>` and never fails.

See `docs/SECURITY.md` for what each guarantees and `docs/PERFORMANCE.md`
for numbers.
