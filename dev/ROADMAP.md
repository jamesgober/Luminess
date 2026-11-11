# Luminess roadmap: 0.2 to 0.10

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
- [x] The syntax in `dev/SYNTAX.md` is frozen (done in 0.9.0, after 0.5.0 had shipped; changes from James's review land in a 0.x release with a migration note).

## 0.6.0: methods and filters

- [x] Method calls with arguments, declared in the schema (`SchemaBuilder::method`) and checked at compile time: `variations.options("size")`; answered by `View::call`, which may return borrowed values or computed text (`Computed`).
- [x] Application filters registered on the builder (`Builder::filter`), typed, and checked like fields.
- [x] More built-in filters: `truncate`, `join`, `first`, `last`, `replace`, `date` (formatting a Unix time), `number` (grouping).
- [x] `{% set name = expr %}` for a local value, scoped to its block.

## 0.7.0: performance

- [x] A render reads the current set without an atomic write (generation counter and a per-thread cache). Under 16 threads the engine's cost over a no-engine control fell from about 1.1-1.5 µs to 0.3-0.6 µs per render (A/B, three alternating runs, loaded machine).
- [x] The HTML escaper scans eight bytes at a time (about 4 GB/s with nothing to escape, up from 1.6); URL escapers copy whole runs; literal runs are merged at compile time.
- [x] Faster template-name lookup (a fast hash in place of SipHash). Pre-resolved `TemplateId` handles were dropped: after the faster hash the lookup is a few nanoseconds, and handles would have to survive reloads.
- [x] `view!` answers fields through a jump table; plain field outputs skip the general evaluator.
- [x] `benches/compare` (its own workspace) renders the same page with Tera, MiniJinja, and Askama. Loop slots stay on the heap: one small allocation per render with loops, below measurement noise.

## 0.8.0: diagnostics and hardening

- [x] Errors with the source line and a caret (`CompileError::report`), and `CompileError::to_html` for development error pages.
- [x] Limits: template size (`Builder::max_template_size`) and output size per render (`Builder::max_output`, `RenderErrorKind::TooLarge`). Nesting, include, and compiled-size limits stay fixed: they guard the stack and compile time, and no real template comes near them.
- [x] `Builder::check` compiles without keeping the result, for editors and CI.
- [x] Long fuzz runs of every target (an hour each for 0.8). The 24-hour runs move to the 1.0 checklist, where they gate the release.

## 0.9.0: freeze

- [x] API review against the simplicity rule: everything public is at the crate root (the `view` module is private now); `FilterFn` stayed internal (`Builder::filter` takes a closure); `TemplateId` was not added.
- [x] Every deferred item and accepted gap revisited. CSS values may now use `rgb rgba hsl hsla calc min max clamp var` (blocking `rgb(...)` was too strict for real themes); comment markers stay blocked. The rest are written down in `docs/SECURITY.md` as accepted limits: regular expressions after `)`, `meta refresh` URLs, `srcset` after the first candidate, template literals nested past 4.
- [x] `docs/` complete: `SYNTAX.md`, `API.md`, `SECURITY.md`, `PERFORMANCE.md`.
- [x] Release candidate for the 1.x API. 1.0 waits for its own decision.

## 0.10.0: a simpler API

- [x] Tier 1 calls that work with plain `?`: `Engine::from_dir`, `Engine::reload_dir`, `Template`, and `luminess::render`.
- [x] One error type (`Error`, `ErrorKind`) for loading, compiling, and rendering.
- [x] Unclear names replaced: `Shaped` is `HasSchema`, `Computed` is `Answer`, `Failed` is `BlockError`, `FieldId` is a plain `usize`; `SchemaRef` is gone.
- [x] Duplicate paths removed: `Templates`, `swap`, `reload(builder)`, and `compile` gave way to `Engine` as the compiled set, with `replace` and `contains`.
- [x] `render` returns a `String`; `render_into` takes a buffer.
- [x] `docs/API.md` in the reference format, every example a doctest; README and crate docs lead with the Tier 1 calls.
- [x] Output byte for byte the same as 0.9.3 (`equivalence/`, `same_output` fuzz target); no slowdown in the render benchmark.

## Before 1.0 (James decides)

- [ ] James reviews `dev/SYNTAX.md` (frozen in 0.9.0); any changes he asks for land first, with a migration note.
- [ ] The web-net integration works end to end: web-net's `Render` trait carries a typed view (proposal sent to the web-net session on 2026-11-08), and Luminess implements it behind an optional `web-net` feature.
- [ ] 24-hour fuzz runs of `compile` and `escape` with no findings.
- [ ] A benchmark run on a quiet machine, and the numbers in the README and `docs/PERFORMANCE.md` updated.
- [ ] One real theme (a site's layouts, partials, and blocks) built on it, and any rough edges it finds fixed.
- [ ] James reviews the 0.10 names (`Engine`, `Template`, `HasSchema`, `Answer`, `BlockError`, `render`/`render_into`, `replace`, `from_dir`/`reload_dir`); docs/API.md ends with why each was chosen.
