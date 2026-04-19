# Typst Fork — Memory Optimization Branch

Fork of `typst/typst` at v0.14.2. The `main` branch carries our memory-reduction
work squashed on top of upstream. Weekly sync with `typst/typst@main` is
automated via `.github/workflows/sync-upstream.yml`.

## Summary of changes vs upstream

- 40+ files changed across `typst-library`, `typst-layout`, `typst-pdf`,
  `typst`, and `typst-cli`.
- PDF output is byte-identical for the standard correctness suite.
- 3380 / 3380 upstream tests pass, plus the 13 tests added for the progress
  plumbing.
- See `BENCHMARKS.md` for peak-RSS numbers vs upstream at 10K–2.4M rows.

## Key modified files

- `crates/typst-library/src/foundations/styles.rs` — Arc-based `Block`.
- `crates/typst-library/src/introspection/tag.rs` — `Tag::Start` carries the
  `Location` inline instead of going through `set_location`.
- `crates/typst-library/src/layout/grid/resolve.rs` — direct cell construction,
  stroke cache, `CellSource`.
- `crates/typst-library/src/engine_flags.rs` — global eviction flag, streaming
  mode flag, cumulative grid-entry counter, heap-compaction helpers.
- `crates/typst-library/src/foundations/target.rs` — `Output::should_stream()`.
- `crates/typst-library/src/progress.rs` — `Sink` trait + `install`/`report`
  for CLI-side progress hooks.
- `crates/typst-layout/src/grid/layouter.rs` — periodic comemo eviction during
  grid layout.
- `crates/typst-layout/src/grid/mod.rs` — lightweight tag cells from
  `CellSource` metadata.
- `crates/typst-layout/src/document.rs` — lazy introspector, `DiskPageStore`,
  `should_stream`.
- `crates/typst-layout/src/page_store/` — `DiskPageStore` with incremental
  append.
- `crates/typst-layout/src/introspect.rs` — incremental
  `PagedIntrospectorBuilder`.
- `crates/typst-layout/src/pages/mod.rs` — Phase-1 spilling, streaming layout,
  eviction.
- `crates/typst-layout/src/flow/mod.rs`, `flow/collect.rs`, `inline/mod.rs` —
  memoize gating for streaming mode.
- `crates/typst-pdf/src/tags/flat.rs` — `FlatTagData` + `ResolvedGroupKind`.
- `crates/typst-pdf/src/tags/groups.rs` — `Groups::flatten()`.
- `crates/typst-pdf/src/tags/tree/build.rs` — `build_from_store` for
  disk-backed tag building.
- `crates/typst-pdf/src/convert.rs` — streaming PDF conversion and
  `convert_streaming`.
- `crates/typst-pdf/src/lib.rs` — `pdf_streaming` entry point.
- `crates/typst-cli/src/compile.rs` — uses `pdf_streaming` for large documents
  and installs the progress sink.
- `crates/typst-cli/src/progress.rs` — stderr-bound `CliSink` that formats
  progress events.
- `crates/typst-cli/src/args.rs` — `--progress` / `-p` and `--verbose` / `-v`
  flags.
- `crates/typst/src/lib.rs` — Phase-2 streaming pass with `StreamingGuard`,
  and the stage/iteration `progress::report` calls.

## Krilla / pdf-writer pins

Tag-serialization work and a smaller per-chunk tweak live on a companion krilla
fork pinned via `Cargo.toml`:

- `krilla = { git = "https://github.com/gpradofe/krilla.git", rev = "..." }`
- `pdf-writer = { git = "https://github.com/gpradofe/krilla.git", rev = "..." }`
  (via `[patch.crates-io]`)

The matching branch on that repo is `streaming-serialize-slim`. Upstream
tracking for that work lives in `LaurenzV/krilla#353`.

## CLI flags added

- `-p` / `--progress` — monotonic percentage + per-page export updates on
  stderr.
- `-v` / `--verbose` — timestamped stage transitions on stderr.
- Combinable.

## Build & test

```
cargo build --release
cargo test --release -p typst-tests    # 3380 / 3380 pass
python ../tests/correctness_test.py    # PDFs byte-identical vs reference
cargo build --release --features dhat-heap   # heap profiling build
```
