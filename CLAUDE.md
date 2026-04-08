# Typst Fork — Memory Optimization Branch

## This is a fork of typst/typst at v0.14.2 with memory optimizations.
## Branch: memory-optimization | Remote: fork (github.com/gpradofe/typst)

## Changes from upstream
- See FINAL_RESULTS.md in project root for full changelog
- 40+ files changed across typst-library, typst-layout, typst-pdf, typst, typst-cli
- All 3376 tests pass, PDF output identical to original

## Key modified files
- `crates/typst-library/src/foundations/styles.rs` — Arc-based Block
- `crates/typst-library/src/introspection/tag.rs` — Tag::Start carries Location
- `crates/typst-library/src/layout/grid/resolve.rs` — Direct cell construction + stroke cache + CellSource
- `crates/typst-library/src/engine_flags.rs` — Global eviction + streaming mode flags
- `crates/typst-library/src/foundations/target.rs` — Output::should_stream() trait method
- `crates/typst-layout/src/grid/layouter.rs` — Periodic comemo eviction during grid layout
- `crates/typst-layout/src/grid/mod.rs` — Lightweight tag cells from CellSource metadata
- `crates/typst-layout/src/document.rs` — Lazy introspector, DiskPageStore, should_stream
- `crates/typst-layout/src/page_store/` — DiskPageStore with incremental append
- `crates/typst-layout/src/introspect.rs` — Incremental PagedIntrospectorBuilder
- `crates/typst-layout/src/pages/mod.rs` — Phase 1 spilling + streaming layout + eviction
- `crates/typst-layout/src/flow/mod.rs` — Memoize gating for streaming mode
- `crates/typst-layout/src/flow/collect.rs` — Memoize gating for streaming mode
- `crates/typst-layout/src/inline/mod.rs` — Memoize gating for streaming mode
- `crates/typst-pdf/src/tags/flat.rs` — FlatTagData + ResolvedGroupKind
- `crates/typst-pdf/src/tags/groups.rs` — Groups::flatten()
- `crates/typst-pdf/src/tags/tree/build.rs` — build_from_store for disk-backed tag building
- `crates/typst-pdf/src/convert.rs` — Streaming PDF conversion + convert_streaming
- `crates/typst-pdf/src/lib.rs` — pdf_streaming function
- `crates/typst-cli/src/compile.rs` — Use pdf_streaming for large documents
- `crates/typst/src/lib.rs` — Phase 2 streaming pass with StreamingGuard

## Build & test
- `cargo build --release` then `cargo test --release -p typst-tests` — 3376/3376 must pass
- `cargo build --release --features dhat-heap` — heap profiling build
- Compare PDF output: `python ../tests/correctness_test.py`
