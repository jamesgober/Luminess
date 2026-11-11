# Fuzzing

Targets:

- `compile`: any bytes as a template through the lexer, parser, compiler, and
  renderer. Nothing may panic; output is UTF-8.
- `same_output`: any bytes as a template, compiled and rendered by this tree
  and by the base release pinned in `Cargo.toml`. Both must agree on whether
  it compiles and produce the same bytes.
- `escape`: any text as a value in HTML text, quoted attributes, a URL, and a
  query component. The value never leaves its place.

Run (nightly and cargo-fuzz):

```bash
cd fuzz
cargo +nightly fuzz run compile seeds/compile -- -max_total_time=600
cargo +nightly fuzz run escape seeds/escape -- -max_total_time=600
```
