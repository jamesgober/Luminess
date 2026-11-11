# Performance

## What a render does

All parsing, name resolution, type checking, and escaper selection happen
once, when the template set is compiled. A render walks the compiled nodes:

- literal text is copied (adjacent pieces were merged at compile time);
- a value is read from the view by the field id the compiler resolved
  (`View::field`, a `match` in code `view!` generates), then written with
  the escaper chosen for its place;
- nothing is converted: no JSON, no maps, no copies of the data.

`Engine::render` takes no lock and makes no shared atomic write: each thread
keeps the engine's current set and checks a generation counter. A reload is
seen by the next render on every thread.

## Measurements

`cargo bench --bench render` (in the crate) and `cargo run --release` in
`benches/compare`. The typical page: a head, a navigation include with 6
links, a 2 KiB trusted HTML body, 20 related posts with tags, a script block
with three values, and a footer; about 10.9 KiB of output and 150 values.

Measured on 2026-11-11 on the shared Windows 11 desktop while it was quiet,
the fastest of seven runs each, with three alternating runs of 0.9.2 and
0.9.3. The ranges are across those three runs.

| Case | 0.9.2 | 0.9.3 |
|---|---|---|
| typical page, reused buffer | 5.1 to 5.3 µs | 3.6 to 4.2 µs |
| typical page into a new `String` | 5.8 to 6.7 µs | 4.1 to 4.7 µs |
| one value, whole render | about 40 ns | about 40 ns |
| compiling the page and its include | 27 to 31 µs | 27 to 29 µs |
| `escape::html`, 4 KiB with nothing to escape | about 0.48 µs (8 GB/s) | about 0.30 µs (12.5 GB/s) |
| `escape::html`, 4 KiB of prose with apostrophes | about 0.74 µs | about 0.62 µs |

Per value, inside a loop of 100 items (the loop itself costs about 5 ns an
item in 0.9.3, 6.6 in 0.9.2):

| Value | 0.9.2 | 0.9.3 |
|---|---|---|
| a field written as is (`raw`) | about 12.5 ns | about 6 ns |
| a 28-byte text field, escaped | about 17 ns | about 11 ns |
| an integer | about 16 ns | about 8 ns |
| a condition on a field (`if p.n`) | about 16 ns | about 6 ns |

| Engine (same page, same machine and day) | Time |
|---|---|
| Askama 0.16 | 1.9 to 2.1 µs |
| **Luminess 0.9.3** | 3.5 to 3.7 µs |
| Tera 2 | 15 to 16 µs |
| MiniJinja 3 | about 26 µs |

Askama generates Rust code from its templates when the program is built; it
cannot reload a theme or compile against schemas known only at startup. Tera
and MiniJinja convert the data through serde on every render. The machine is
usually busy with other work; numbers taken under load (as in earlier
releases' notes) are about twice these.

## Where the time goes

A plain field output (no filters, no method calls) is compiled to its own
node that reads the field path directly and writes text, URLs, and integers
without the general evaluator; a literal followed by such an output is one
node; a condition on a plain field takes the same direct path. What remains
per value is the view's own `field` call (a `match` in `view!` code, through
a trait object) and the escaper. Escaping skips sixteen bytes at a time
while nothing needs escaping, then eight, and copies runs of safe bytes at
once; integers are written two digits per step. The output buffer is grown
once, from the template's literal size, before a render starts.

Askama is still about 1.8 times faster on this page: its templates are Rust
functions, so each value is a direct field access the compiler can inline.
Luminess keeps the template as data, which is what makes live reload and
schemas known only at startup possible; the remaining cost is mostly that
indirection.

Tried and dropped in 0.9.3 (gains inside noise): checking the last bytes of a
string as one overlapping eight-byte word, instead of byte by byte.

## Allocation

A render allocates nothing for templates without loops, and one small vector
for the loop slots of templates with loops. Filters that build text
(`upper`, `join`, `date`, ...) allocate their result. Write into a buffer you
keep (`out.clear()` between renders) to reuse its capacity.
