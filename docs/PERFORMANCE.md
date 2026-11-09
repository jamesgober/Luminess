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

| Case | Time |
|---|---|
| typical page | about 9 µs |
| one value | about 60 ns |
| compiling the page and its include | about 40 µs |
| `escape::html`, 4 KiB with nothing to escape | about 1 µs (4 GB/s) |
| `escape::html`, 4 KiB of prose with apostrophes | about 1.5 µs |

| Engine (same page) | Time |
|---|---|
| Askama 0.16 | about 4 µs |
| **Luminess** | about 9 µs |
| Tera 2 | about 28 µs |
| MiniJinja 3 | about 45 µs |

Askama generates Rust code from its templates when the program is built; it
cannot reload a theme or compile against schemas known only at startup. Tera
and MiniJinja convert the data through serde on every render.

The machine is a shared Windows 11 desktop that is usually busy with other
work, so each number is the fastest of seven runs, and comparisons were made
by alternating runs of the two builds. Expect lower numbers on a quiet
machine.

## Where the time goes

On the typical page, roughly a third is escaping text, a third is reading and
dispatching values (about 20 ns per value), and the rest is copying literals
and looping. Escaping scans eight bytes at a time and copies runs of safe
bytes at once.

## Allocation

A render allocates nothing for templates without loops, and one small vector
for the loop slots of templates with loops. Filters that build text
(`upper`, `join`, `date`, ...) allocate their result. Write into a buffer you
keep (`out.clear()` between renders) to reuse its capacity.
