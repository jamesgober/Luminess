# Luminess roadmap: 0.2 to 0.9

Every version ends the same way: fuzzing of the lexer and parser (the
`compile` and `escape` targets), property tests for every escaper it touches,
the render benchmark, then a pull request, green CI, a tag, a GitHub
pre-release, crates.io, and docs.rs. 1.0 is not on this list; it is decided
separately.

Performance target throughout: a typical page (about 10 KiB of output, 150
values, one include, two loops) renders in 2 to 20 µs. `benches/render.rs`
measures it.

## 0.2.0: the engine

The rebuild. Nothing of 0.1's API remains.

- [x] Lexer: `{{ }}`, `{% %}`, `{# #}`, `{% raw %}`, `-` trim markers, strings that hold closing delimiters.
- [x] Parser: `if` / `elif` / `else`, `for` / `else`, `include`, `try` / `fallback`; expressions with `not`, `and`, `or`, comparisons, literals, parentheses, filters. Nesting is bounded (64), so no template exhausts the stack.
- [x] Typed views: `View`, `Value`, `List`, `Schema`, `Kind`, `Shaped`, and the `view!` macro. Field names resolve to ids at compile time.
- [x] Compile errors at startup for unknown names, fields, filters, templates, and type mismatches, with line, column, and a suggestion. All errors of a set are reported together.
- [x] Contextual escaping, picked at compile time: HTML text, quoted attribute values, `<title>` and `<textarea>`, URL attributes (scheme check at the start, normalized in the path, component-encoded in the query). Unquoted attribute values, tag and attribute names, comments, `srcdoc`, scripts, and styles are compile errors.
- [x] Branches and loop bodies must end in the same HTML context; URL parts join to the stricter one.
- [x] `| raw`, HTML text only. Trusted `html` fields.
- [x] Blocks that render themselves; a failing block is rolled back and its fallback written; `try` / `fallback` for custom markup; `Rendered` reports the failures.
- [x] `Engine` with `reload` and `swap` (live reload; a set with errors is refused); `Templates` snapshots.
- [x] Filters: `default`, `upper`, `lower`, `trim`, `length`, `raw`.
- [x] Zero dependencies; MSRV 1.85.

## 0.3.0: scripts and styles

- [x] Output inside `<script>` and `on*` attributes: a JavaScript literal in code, escaped content inside an existing string; string, comment, regular-expression, and template-literal tracking.
- [x] Output inside `<style>` and `style` attributes: checked values in code (anything that could start a function, such as `url(`, is blocked), hex escaping in strings; strings and comments tracked.
- [x] Numbers, bools, and absent values as JavaScript literals. (An explicit `| js` filter turned out unnecessary: the context already picks it.)
- [x] Character references in `on*` and `style` attribute text stop output there (they change the script after decoding).
- [x] A template that ends inside a `<script>`, `<style>`, comment, attribute, or tag is an error.
- [x] Property tests and fuzzing for both escapers.

## 0.4.0: composition

- [x] Layouts: `{% extends "base" %}` with `{% block name %}...{% endblock %}`, `{{ super() }}`, multi-level chains, nested blocks, checked at compile time (unknown block names, text outside blocks, cycles, self-containing blocks).
- [x] `{% include "x" with item %}`: the partial sees one value as its root, checked against that value.
- [x] Template sources from a folder (`Builder::directory`), names from relative paths; `engine.reload(Engine::builder().directory(..)?)` reloads a theme.
- [x] Per-template schemas inside one set, documented for multi-type sites.

## 0.5.0: blocks, fallbacks, and the syntax freeze

- [x] Block fallbacks declared in the template: `{{ page.stock | fallback("Unavailable") }}`.
- [x] `Rendered::failures` lists every failure (up to `Rendered::MAX_FAILURES`), not only the first.
- [x] A block can go in an attribute, URL, script, or style through its text form (`View::text`), escaped for the place.
- [ ] The syntax in `dev/SYNTAX.md` is reviewed and frozen; changes after this are additions only.

## 0.6.0: methods and filters

- [ ] Method calls with arguments, declared in the schema and checked at compile time: `variations.options("size")`, `money.format("EUR")`.
- [ ] Application filters registered on the builder, typed, and checked like fields.
- [ ] More built-in filters: `truncate`, `join`, `first`, `last`, `replace`, `date` (formatting a Unix time), `number` (grouping).
- [ ] `{% set name = expr %}` for a local value.

## 0.7.0: performance

- [ ] A render reads the current set without an atomic write (generation counter and a per-thread cache), measured under 16 threads.
- [ ] Escapers that scan eight bytes at a time; literal runs merged; loop slots on the stack.
- [ ] Pre-resolved template handles (`TemplateId`) so a server skips the name lookup.
- [ ] A benchmark crate (`benches/compare`, its own workspace) against Tera, MiniJinja, and Askama on the same page.

## 0.8.0: diagnostics and hardening

- [ ] Errors with the source line and a caret, and a `CompileError` rendering for development error pages.
- [ ] Limits: template size, output size per render (optional), include fan-out, all configurable.
- [ ] A `check` API that compiles without keeping the result, for editors and CI.
- [ ] 24-hour fuzz runs of every target.

## 0.9.0: freeze

- [ ] API review against the simplicity rule; anything unclear is renamed or removed.
- [ ] Every deferred item and accepted gap revisited; performance tuned again.
- [ ] `docs/` complete: API, syntax, security model (what each escaper guarantees), performance.
- [ ] Release candidate. 1.0 waits for its own decision.
