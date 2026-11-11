# Changelog

All notable changes to `luminess` are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.10.0] - 2026-11-11

A simpler API. Breaking: every change is listed under "Migrating from 0.9"
below. Template syntax, escaping, and output are unchanged byte for byte,
and rendering is as fast as in 0.9.3.

### Added

- `Engine::from_dir(dir)`: a whole theme in one call, every `.html` file
  below `dir`, named by its path without the extension.
- `Engine::reload_dir(dir)`: compile the folder again and swap it in; on any
  error the running theme stays.
- `Template<V>`: one compiled template (`Template::new(source)`,
  `render(&view)`, `render_into(&view, &mut out)`) for cards, emails, and
  fragments.
- `luminess::render(source, &view)`: compile and render in one call, for
  scripts and tests.
- `Engine::replace(engine)` and `Engine::contains(name)`: build and check a
  new theme fully, then swap it in.
- `Builder::directory_with_extension(dir, extension)`.
- `ErrorKind::Io`: a theme folder or file that cannot be read.
- `docs/API.md` rewritten: every public item with its parameters, errors,
  and examples, simplest first; every example runs as a doctest.

### Changed

- One error type, `Error`, for loading, compiling, and rendering, so `?`
  works from `from_dir` to `render`. `Error::kind()` says which
  (`ErrorKind` is `#[non_exhaustive]`); `diagnostics()`, `report()`, and
  `to_html()` are on it for every kind.
- `Engine::render` returns the page as a `String`, the common case;
  `render_into` appends to a buffer you keep.
- Views take a field's position as a plain `usize` instead of `FieldId`.
- Renamed: `Shaped` to `HasSchema`, `Computed` to `Answer`, `Failed` to
  `BlockError`, `template_with` to `template_with_schema`, `builder_with`
  to `with_schema`.
- `SchemaBuilder::object` and `Kind::Object` take a `Schema`;
  `Kind::object_of::<T>()` still builds a type's schema the first time it
  is needed, so types can refer to themselves.
- The equivalence tests and the `same_output` fuzz target compare against
  0.9.3.

### Removed

- `Templates`, `Engine::new`, `Engine::templates`, `Engine::swap`,
  `Engine::reload`, `Builder::new`, `Builder::compile`: an `Engine` is the
  compiled set, and `replace` swaps one in.
- `CompileError`, `RenderError`, `RenderErrorKind`, `FieldId`, `SchemaRef`.
- `Rendered::is_complete` and `Rendered::first_failure`: use `failures()`.

### Migrating from 0.9

| 0.9 | 0.10 |
|---|---|
| `Engine::<V>::builder().directory(dir, "html")?.build()?` | `Engine::<V>::from_dir(dir)?` |
| `Builder::directory(dir, "html")` (an `io::Error`) | `Builder::directory(dir)` (a `luminess::Error`) |
| `Builder::directory(dir, "txt")` | `Builder::directory_with_extension(dir, "txt")` |
| `engine.render(name, &view, &mut out)` | `engine.render_into(name, &view, &mut out)` |
| `engine.render_to_string(name, &view)` | `engine.render(name, &view)` |
| `engine.reload(builder)` | `engine.replace(builder.build()?)`, or `engine.reload_dir(dir)` |
| `builder.compile()` and `Engine::new(templates)` | `builder.build()` |
| `engine.templates()`, `Templates`, `engine.swap(templates)` | an `Engine` is the set: `engine.replace(other)`, `engine.contains(name)` |
| `Builder::new(schema)`, `Engine::builder_with(schema)` | `Engine::with_schema(schema)` |
| `Builder::template_with(name, source, schema)` | `Builder::template_with_schema(name, source, schema)` |
| `CompileError`, `RenderError`, `RenderErrorKind` | `Error` and `ErrorKind` (`Compile`, `Io`, `NotFound`, `TooLarge`) |
| `RenderError::template()`, `CompileError::diagnostics()` | `Error::template()`, `Error::diagnostics()` |
| `Shaped` | `HasSchema` |
| `Computed` (`Computed::Value`, `Computed::Text`) | `Answer` (`Answer::Value`, `Answer::Text`) |
| `Failed` | `BlockError` |
| `fn field(&self, id: FieldId)` with `match id.index()` | `fn field(&self, index: usize)` with `match index` |
| `fn call(&self, id: FieldId, args)` | `fn call(&self, index: usize, args)` |
| `Schema::field(name) -> Option<(FieldId, &Kind)>` | `Schema::field(name) -> Option<(usize, &Kind)>` |
| `SchemaRef`, `SchemaBuilder::object(name, impl Into<SchemaRef>)` | `SchemaBuilder::object(name, Schema)`; `Kind::Object(Schema)` |
| `Rendered::is_complete()` | `rendered.failures().is_empty()` |
| `Rendered::first_failure()` | `rendered.failures().first()` |

A view written by hand, before and after:

```rust
// 0.9
impl View for Post {
    fn field(&self, id: FieldId) -> Value<'_> {
        match id.index() { 0 => Value::Text(&self.title), _ => Value::Absent }
    }
}
// 0.10
impl View for Post {
    fn field(&self, index: usize) -> Value<'_> {
        match index { 0 => Value::Text(&self.title), _ => Value::Absent }
    }
}
```

Views made with `view!` need no change.

## [0.9.3] - 2026-11-11

A security release, and faster rendering. Upgrade from any earlier 0.9 or
older version: no API changes, no template changes for correct themes.

### Security

- Fixed URL scheme checking for URLs built from several values, or from a
  value and the template's own text, and for whitespace before a value in a
  URL attribute. A value written where a URL's scheme may still be open has
  `:` percent-encoded; a literal `:` or `&` right after such a value is a
  compile error.
- Comment and raw-text element parsing now follows the HTML specification:
  every way a comment can end, and `xmp`, `iframe`, `noembed`, `noframes`,
  `noscript`, and `plaintext` as raw text.
- The JavaScript tracker reads HTML-like comments, line breaks in strings,
  line separators in comments, and `/` after `++` and `--` as browsers do.
- A long chain of `and`, `or`, or filters is a compile error instead of
  exhausting the stack.
- Compiling many includes is linear in time, and a template is limited to
  16 MiB of text once its includes are inlined.
- A template added with `Builder::template_with` whose schema the engine's
  view type does not answer can no longer be rendered by name, only
  included.
- Branches that end in the same JavaScript string now join instead of
  being refused.

### Changed

- A typical page renders about 30% faster (3.6 to 4.2 µs, from 5.1 to
  5.3 µs on the same machine). A plain field output costs about 6 ns of
  dispatch instead of 12.5, a condition on a field about 6 ns instead of 16,
  and an integer about 8 ns instead of 16.
- Plain field outputs and conditions read their path directly; a literal and
  the output after it are walked as one node; text, URLs, and integers are
  written without the general evaluator; integers two digits per step.
- The output buffer is reserved once from the template's literal size,
  which saves about 1.5 µs a page when rendering into a new `String`.
- `escape::html` skips sixteen bytes at a time on text with nothing to
  escape (about 12.5 GB/s, from 8).

### Added

- `equivalence/`: property tests that this tree renders what the published
  base release renders, and the `same_output` fuzz target doing the same on
  any input. Both run in CI. Against 0.9.2 they allow differences only where
  the security fixes above apply, and check there that no `javascript:` URL
  is written.

## [0.9.2] - 2026-11-09

### Changed

- The `web-net` feature accepts web-net 0.8 and 0.9 (`>=0.8, <0.10`); the
  `Render` trait is the same in both. CI tests the oldest and the newest.

## [0.9.1] - 2026-11-09

### Added

- The `web-net` feature (off by default; the default build still depends on
  nothing): `Engine<V>` implements `web_net::Render` with the application's
  own view type, and `luminess::web::Theme` adds error pages from an
  `Engine<ErrorView>` (`error/<status>`, then `error`). Template ids
  resolved at startup stay valid across reloads, and rendering by id reads
  one array entry with no shared write.

## [0.9.0] - 2026-11-08

API freeze for 1.x and the pre-1.0 polish pass.

### Changed

- CSS values may use the functions `rgb`, `rgba`, `hsl`, `hsla`, `calc`,
  `min`, `max`, `clamp`, and `var` around plain text, and `+ * /` (sizes
  worked out with `calc`). Any other function, `url(` included, and CSS
  comment markers are still replaced with `blocked`.
- The `view` module is private; everything public was already at the crate
  root.

- The template syntax is frozen: later changes come with a migration note.

### Added

- `docs/SYNTAX.md`, `docs/API.md`, `docs/SECURITY.md` (what each escaper
  guarantees, and the known limits), and `docs/PERFORMANCE.md`.

## [0.8.0] - 2026-11-08

Diagnostics and limits.

### Added

- `CompileError::report`: every problem in the style of compiler errors,
  with the source line and a caret under the column.
- `CompileError::to_html`: the report as an escaped `<pre>` block for
  development error pages.
- `Diagnostic::source_line`.
- `Builder::max_output(bytes)`: a render that would write more stops,
  removes its output, and fails with the new `RenderErrorKind::TooLarge`.
- `Builder::max_template_size(bytes)` (1 MiB by default): a larger source is
  a compile error.
- `Builder::check`: compiles every template and reports the errors without
  consuming the builder or keeping the result.

### Fixed

- Nested `for` loops took time exponential in their depth to compile: each
  body was compiled twice, so 20 nested loops took about a second and the
  allowed 64 would never finish (found by fuzzing). A body is now compiled
  again only when it ends in a different HTML context than it starts in, and
  both passes count toward the compiled-size limit.

## [0.7.0] - 2026-11-08

Performance.

### Changed

- `Engine` renders without a lock or a shared atomic write: each thread keeps
  the engine's current template set and checks a generation counter, so
  renders on many threads no longer contend. `reload` and `swap` are seen by
  the next render on every thread.
- `escape::html` scans eight bytes at a time (about 2.5x faster on text with
  nothing to escape); `escape::url` and `escape::url_component` copy runs of
  safe bytes at once.
- Template names are looked up with a fast hash.
- `view!` answers fields through a jump table; outputs that are plain field
  paths skip the general evaluator.

### Added

- `benches/compare`: the typical-page benchmark rendered by Luminess, Askama,
  Tera, and MiniJinja.

## [0.6.0] - 2026-11-08

Methods and filters.

### Added

- Methods: `SchemaBuilder::method(name, params, returns)` declares a value
  computed from arguments; templates call it as `x.options("size")` or
  `x.cheapest()`, with the argument count and kinds checked at compile time.
  `View::call` answers with a `Computed`: a borrowed `Value` or text built
  for the call. Calling a field or naming a method without `()` is a compile
  error with a hint.
- Application filters: `Builder::filter(name, params, returns, closure)`,
  checked at compile time like built-in ones.
- Built-in filters `truncate(n)`, `join(sep)`, `first`, `last`,
  `replace(from, to)`, `date(format)` (Unix seconds, UTC), and `number`
  (digit grouping).
- `{% set name = expr %}`: a local value until the end of its block.

## [0.5.0] - 2026-11-08

Blocks and fallbacks.

### Added

- `{{ block | fallback("text") }}`: the text written when a block that renders
  itself fails, in place of `View::fallback`.
- `View::text`: a block's plain-text form. A block may now be written in an
  attribute value, a URL, a script, or a style; its text form is escaped for
  the place. Without one, the fallback text (or nothing) is written and the
  failure is reported.
- `Rendered::failures` lists the reasons of every block failure, up to
  `Rendered::MAX_FAILURES` (16).

### Changed

- Outputting a block outside HTML text is no longer a compile error; it uses
  the text form.

## [0.4.0] - 2026-11-08

Composition.

### Added

- Layouts: `{% extends "base" %}`, `{% block name %}...{% endblock %}` (the
  closing tag may repeat the name), and `{{ super() }}`. Chains may be up to
  8 templates deep and blocks may nest. Compile errors for an unknown parent,
  a cycle, a block the chain does not have (with a suggestion), text or
  statements outside blocks in an extending template, `extends` that is not
  first, a block defined twice, `super()` without a parent version, and a
  block that would contain itself.
- `{% include "name" with value %}`: the partial sees `value` as its root,
  named by its own schema, and is checked against it.
- `Builder::directory(dir, extension)`: adds every matching file below a
  folder, named by its relative path without the extension.

## [0.3.0] - 2026-11-08

Scripts and styles.

### Added

- Output in JavaScript: in `<script>` elements and `on...` attributes. In
  code a value becomes a complete literal (text as a double-quoted string,
  numbers, `true`/`false`, absent as `null`); inside a string or template
  literal its content is escaped. The compiler follows strings, template
  literals with `${}`, comments, and regular expressions.
- Output in CSS: in `<style>` elements and `style` attributes. A value in code
  passes when it holds only letters, digits, spaces, and `# % . , - _`, and is
  replaced with `blocked` otherwise; inside a CSS string it is hex-escaped.
- `escape::js_string`, `escape::js_quoted`, `escape::css`,
  `escape::css_value_allowed`, `escape::BLOCKED_CSS`.
- Compile errors for output in a JavaScript comment or regular expression,
  right after `$` in a template literal, in a CSS comment, and in an `on...`
  or `style` attribute whose text uses character references.
- A template that ends inside a tag, attribute value, comment, script, or
  style is a compile error.
- Property tests and fuzzing for the new escapers.

## [0.2.0] - 2026-11-08

A rebuild. Nothing of the unpublished 0.1 API remains.

### Added

- Templates compile at startup against typed views: `View`, `Value`, `List`,
  `Schema`, `Kind`, `Shaped`, and the `view!` macro. Field names resolve to
  ids at compile time.
- Jinja-like syntax: `{{ }}`, `{% if %}` / `elif` / `else`, `{% for %}` with
  `else` and `loop` values, `{% include %}`, `{% try %}` / `fallback`,
  comments, `{% raw %}`, and `-` whitespace control. Conditions with `not`,
  `and`, `or`, comparisons, and literals. Filters `default`, `upper`, `lower`,
  `trim`, `length`, and `raw`.
- Compile errors for unknown names, fields, filters, and templates, type
  mismatches, and unsafe output places, with line, column, and suggestions.
  Every error of a set is reported at once.
- Contextual escaping chosen at compile time for HTML text, quoted attribute
  values, `<title>` and `<textarea>`, and URL attributes (scheme check,
  normalizing, component encoding). The escapers are public in `escape`.
- Blocks that render themselves (`View::render`), with rollback and
  `View::fallback` when they fail; `Rendered` reports failures.
- `Engine` with live reload (`reload`, `swap`) and `Templates` snapshots.
- Fuzz targets, property tests for escaping, and a render benchmark.

### Removed

- The 0.1 engine and its dependencies on `serde`, `serde_json`, and
  `thiserror`. Luminess has no dependencies.
