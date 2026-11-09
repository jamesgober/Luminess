# Changelog

All notable changes to `luminess` are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
