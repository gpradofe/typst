# Typst Memory Optimization Benchmarks

Comprehensive benchmarks comparing the **original Typst 0.14.2** binary against the **optimized fork** at commit `bdaeb6b69` on the `main` branch. All measurements are real profiling data collected on the same machine, back-to-back, with no other heavy processes running.

Data captured: **2026-04-22**.

## Key Results

### At 100,000 rows

| Metric | Original | Optimized | Delta |
|--------|----------|-----------|-------|
| **Simple Table** — Peak RAM | 16,086 MB | **449 MB** | **97.2 % reduction** |
| **Simple Table** — Time | 79.8 s | **19.8 s** | **4.0 × faster** |
| **Single Table (Advanced)** — Peak RAM | 15,491 MB | **563 MB** | **96.4 % reduction** |
| **Single Table (Advanced)** — Time | 78.1 s | **47.1 s** | **1.7 × faster** |
| **Multi-Table (Advanced)** — Peak RAM | 14,711 MB | **687 MB** | **95.3 % reduction** |
| **Multi-Table (Advanced)** — Time | 62.0 s | **41.5 s** | **1.5 × faster** |

### At 10,000 rows

| Metric | Original | Optimized | Delta |
|--------|----------|-----------|-------|
| Simple Table — Peak RAM | 1,696 MB | **52 MB** | **96.9 % reduction** |
| Simple Table — Time | 7.7 s | **1.3 s** | **5.9 × faster** |
| Single Table (Advanced) — Peak RAM | 1,625 MB | **101 MB** | **93.8 % reduction** |
| Single Table (Advanced) — Time | 4.9 s | **2.9 s** | **1.7 × faster** |
| Multi-Table (Advanced) — Peak RAM | 1,607 MB | **175 MB** | **89.1 % reduction** |
| Multi-Table (Advanced) — Time | 3.9 s | **2.8 s** | **1.4 × faster** |

### At 1.2 million rows (optimized only — original exceeds 128 GB)

| Template | Peak RAM | Time | PDF size | RAM / PDF ratio |
|----------|---------:|-----:|---------:|----------------:|
| Simple            | **5,498 MB** | 5.4 min | 2,549 MB | **2.16 ×** |
| Single Table (Advanced) | **6,335 MB** | 12.1 min | 3,333 MB | **1.90 ×** |
| Multi-Table (Advanced)  | **6,804 MB** | 10.2 min | 3,309 MB | **2.06 ×** |

**Peak RAM is now within ~2 × the final PDF size** for every production workload at 1.2 M rows. The remaining headroom is dominated by krilla's in-memory PDF assembly buffer (`pdf_writer::Buf::with_capacity` holds ~3 GB at 1.2 M simple — tracked upstream at [LaurenzV/krilla#353](https://github.com/LaurenzV/krilla/issues/353)).

## Scaling summary

| Rows | Simple RAM | Single-Adv RAM | Multi-Table RAM |
|-----:|-----------:|---------------:|----------------:|
| 10 K | 52 MB | 101 MB | 175 MB |
| 100 K | 449 MB | 563 MB | 687 MB |
| 1.2 M | 5,498 MB | 6,335 MB | 6,804 MB |

100 K → 1.2 M scaling factor averages **~11 ×** (versus 12 × data growth), i.e. memory scales slightly sublinearly.

## What was optimized

See [`FORK_NOTES.md`](FORK_NOTES.md) for the full list of modified files across `typst-library`, `typst-layout`, `typst-pdf`, `typst`, and `typst-cli`. The key techniques:

1. **`Location` moved from `Content` to `Tag`** — eliminates `make_unique()` deep clones on every table cell.
2. **Direct cell construction in `resolve_cell`** — avoids `RawContent::clone_impl()` overhead per cell.
3. **Thread-local stroke deduplication cache** — identical table strokes share a single `Arc`.
4. **Periodic comemo eviction during grid layout** — frees completed page caches every N pages.
5. **`DiskPageStore` streaming** — pages serialized to disk after runs of >100 pages; only recent pages in memory.
6. **Flat PDF tag tree** — `FlatTagData` with parallel arrays replaces the allocation-heavy tree representation at tag-resolve time.
7. **Krilla/pdf-writer fork** — consuming tag serialization + slim chunk container.

All optimizations preserve **visually and textually identical PDF output** (verified by `tests/correctness_test.py`). Byte-level differences are limited to PDF metadata and structure IDs, not rendered content.

## Methodology

### Measurement
- **Peak RAM**: `psutil.Process.memory_info().rss` polled every 100 ms, plus all child processes. OS-visible peak RSS, not dhat-reported bytes-at-peak.
- **Time**: Wall-clock time around the full compile + export command.
- **PDF size**: `os.path.getsize()` on the output PDF.

### Environment
- **OS**: Windows 11 Pro (10.0.26100)
- **CPU**: Intel Core i9-14900K (32 threads)
- **RAM**: 128 GB DDR5
- **Storage**: NVMe SSD
- **Python**: 3.12 with psutil

### Binaries
- **Original**: `typst-bin/typst-x86_64-pc-windows-msvc/typst.exe` — Typst 0.14.2 official release.
- **Optimized**: `target/release/typst.exe` built from this fork at commit `bdaeb6b69`.

### Reproducibility

```bash
cargo build --release
cargo test --release -p typst-tests           # 3,380 pass
python ../tests/correctness_test.py           # PDFs visually identical
python ../tests/_measure_optv2_rss.py         # RSS matrix for 3 templates × 3 sizes
```

Requirements: `pip install psutil matplotlib numpy`.

### Raw data

- Ground-truth RSS matrix: `../docs/plans/opt-v2-measurements/rss_summary.json`
- Original-binary matched matrix: `../docs/plans/opt-v3-measurements/original_binary_matrix.json`
- 100 K dhat per-site: `../docs/plans/opt-v2-measurements/measurements.md`
- 10 K + 100 K CPU traces analyzed: `../docs/plans/opt-v3-measurements/timings.md`
