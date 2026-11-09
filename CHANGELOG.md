# Changelog

All notable changes to `luminess` are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
