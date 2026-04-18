# Typst Memory Optimization Benchmarks

Comprehensive benchmarks comparing the **original Typst 0.14.2** binary against the **optimized fork** with memory-reduction and speed patches. All measurements are real profiling data collected on the same machine under consistent conditions.

## Key Results

At **100,000 rows** (the largest size practical for both binaries):

| Metric | Original | Optimized | Improvement |
|--------|----------|-----------|-------------|
| **Simple Table** — Peak RAM | 16,087 MB | 380 MB | **98% reduction** |
| **Simple Table** — Time | 41.8s | 14.5s | **2.9x faster** |
| **Single Table (Advanced)** — Peak RAM | 15,491 MB | 564 MB | **96% reduction** |
| **Single Table (Advanced)** — Time | 44.8s | 33.5s | **1.3x faster** |
| **Multi-Table (Advanced)** — Peak RAM | 14,706 MB | 689 MB | **95% reduction** |
| **Multi-Table (Advanced)** — Time | 36.4s | 30.9s | **1.2x faster** |

At **10,000 rows** the stress template (8 complex per-department tables with gradients, badges, math equations) goes from **4,585 MB / 14.7 s** down to **503 MB / 12.0 s** — an **89 %** RAM reduction with a 1.2× speedup.

At **600,000 rows**, the original binary requires **~90 GB of RAM** while the optimized binary uses **2.7-3.4 GB** — a **96 %** reduction. Speedup ranges from **1.4× (multi-table)** to **4.7× (simple)** at this scale. The optimized binary further scales to **1.2 million rows** (producing 3+ GB PDFs) at peak RAM **5.5-6.8 GB** — sizes the original binary cannot handle (projected ~180 GB).

## Overview

<p align="center">
  <img alt="Summary" src="benchmarks/summary.png" width="900">
</p>

## Graphs

### Peak Memory Usage

Log-log comparison showing peak RSS across all row counts. Dashed lines = original, solid = optimized. The gap between the curves represents the memory savings — consistently 75-85% at scale.

![Memory Comparison](benchmarks/memory_comparison.png)

### Memory Reduction Percentage

How much RAM the optimized binary saves at each data size. Reductions grow with scale, reaching **75-85%** at 100K rows.

![Memory Reduction](benchmarks/memory_reduction.png)

### Compilation Speed

The optimized binary is consistently faster, with speedup increasing at larger sizes. Simple tables see the biggest benefit (**2.4x** at 100K rows).

![Speedup](benchmarks/speedup.png)

### Time Comparison

Log-log plot of wall-clock compilation time for all templates and sizes.

![Time Comparison](benchmarks/time_comparison.png)

### Side-by-Side Comparison

Direct comparison at key data sizes. Faded bars = original, solid = optimized.

![Bar Comparison](benchmarks/bar_comparison.png)

### Scaling Efficiency

RAM and time per 1,000 rows. The optimized binary uses **~23 MB/1K rows** vs the original's **~160 MB/1K rows** — a 7x efficiency improvement.

![Scaling Efficiency](benchmarks/scaling.png)

### Optimized Binary: Large Document Scaling

The optimized binary scales to 1.2M rows, producing 3+ GB PDFs. Scaling is approximately linear up to 600K rows; beyond that, time grows super-linearly due to memory pressure at 27-40 GB RSS.

![Optimized Scaling](benchmarks/optimized_scaling.png)

## Full Results Table

### Original vs Optimized (100 to 100K rows)

| Template | Rows | Orig RAM (MB) | Opt RAM (MB) | RAM Saved | Orig Time | Opt Time | Speedup |
|----------|------|---------------|--------------|-----------|-----------|----------|---------|
| Simple | 100 | 26 | 19 | 27% | 0.2s | 0.2s | 1.0x |
| Simple | 1,000 | 176 | 25 | 86% | 0.5s | 0.3s | 1.7x |
| Simple | 10,000 | 1,671 | 55 | 97% | 3.8s | 1.4s | 2.7x |
| Simple | 50,000 | 8,281 | 181 | 98% | 20.3s | 7.2s | 2.8x |
| Simple | 100,000 | 16,087 | 380 | 98% | 41.8s | 14.5s | 2.9x |
| Single Adv. | 100 | 30 | 16 | 47% | 0.3s | 0.2s | 1.5x |
| Single Adv. | 1,000 | 208 | 49 | 76% | 0.5s | 0.3s | 1.7x |
| Single Adv. | 10,000 | 1,641 | 103 | 94% | 4.0s | 3.2s | 1.2x |
| Single Adv. | 50,000 | 7,830 | 303 | 96% | 20.6s | 16.7s | 1.2x |
| Single Adv. | 100,000 | 15,491 | 564 | 96% | 44.8s | 33.5s | 1.3x |
| Multi-Table | 100 | 23 | 14 | 39% | 0.2s | 0.2s | 1.0x |
| Multi-Table | 1,000 | 184 | 68 | 63% | 0.5s | 0.4s | 1.2x |
| Multi-Table | 10,000 | 1,615 | 182 | 89% | 3.6s | 3.2s | 1.1x |
| Multi-Table | 50,000 | 7,528 | 414 | 95% | 17.4s | 15.9s | 1.1x |
| Multi-Table | 100,000 | 14,706 | 689 | 95% | 36.4s | 30.9s | 1.2x |

### Large Scale (300K–600K rows, Original vs Optimized)

| Template | Rows | Orig RAM (MB) | Opt RAM (MB) | RAM Saved | Orig Time | Opt Time | Speedup |
|----------|------|---------------|--------------|-----------|-----------|----------|---------|
| Simple | 300,000 | 45,160 | 1,367 | 97% | 151.0s | 46.9s | 3.2x |
| Simple | 600,000 | 89,972 | 2,741 | 97% | 471.3s | 101.3s | 4.7x |
| Single Adv. | 300,000 | 45,482 | 1,621 | 96% | 193.6s | 111.1s | 1.7x |
| Single Adv. | 600,000 | 89,862 | 3,198 | 96% | 965.4s | 231.5s | 4.2x |
| Multi-Table | 300,000 | 41,949 | 1,761 | 96% | 115.6s | 97.0s | 1.2x |
| Multi-Table | 600,000 | 81,591 | 3,388 | 96% | 285.4s | 203.7s | 1.4x |

### Optimized-Only (1.2M rows)

| Template | Rows | Optimized RAM (MB) | Optimized Time |
|----------|------|--------------------|----------------|
| Simple | 1,200,000 | 5,499 | 352.6s |
| Single Adv. | 1,200,000 | 6,322 | 713.0s |
| Multi-Table | 1,200,000 | 6,806 | 414.4s |

The original binary was not tested at 1.2M rows due to projected memory requirements (~180 GB).

> **Note on scaling:** For the original binary, RAM scales approximately linearly, peaking at ~90 GB for simple/advanced tables at 600K rows. Time scaling is super-linear for single-table-advanced (5x from 300K→600K) due to the convergence loop operating over very large pages.
>
> For the optimized binary, RAM scales linearly from 10K all the way to 1.2M (a perfectly linear ~4.5 MB / 1K rows on simple, ~5 MB / 1K rows on single-advanced, ~5.5 MB / 1K rows on multi-table). Time scales linearly up to 600K and slightly super-linearly at 1.2M due to allocator pressure at 5-7 GB RSS.
>
> Multi-Table now matches single-table performance across all scales: the chunked parallel layout (default 2 concurrent runs with comemo eviction between chunks) bounds peak memory to roughly the largest individual table's footprint without sacrificing rayon speedup.

## Test Templates

Three templates test different real-world table patterns:

### 1. Simple Table (`table_test.typ`)
- Plain 10-column table with no styling
- Single continuous `#table()` element
- Columns: ID, Name, Email, Department, Role, Salary, Start Date, Office, Phone, Status
- Data format: flat JSON array

### 2. Single Table Advanced (`single_table_advanced_test.typ`)
- One continuous table spanning thousands of pages
- Group header rows within the table for department/team transitions
- Page headers and footers with page numbers ("Page X of Y")
- Alternating row fills, styled borders, 14 columns
- Data format: grouped JSON with departments and teams

### 3. Multi-Table (`advanced_table_test.typ`)
- Separate `#table()` for each department/team group
- Each table has its own header row
- Page headers and footers
- Alternating row fills, styled borders
- Simulates a real business report PDF
- Data format: same grouped JSON as single-table-advanced

## What Was Optimized

The optimized binary combines memory reduction and speed improvements applied across Typst's layout, tagging, and PDF-export pipeline. Memory changes (original → current RSS) are preserved while speed changes layer on top.

### Memory reductions
1. **Eliminated deep cloning in `Content::set()`** — Moved `Location` from Content to `Tag` to avoid triggering `make_unique()` deep copies on every cell
2. **Fresh cell construction in `resolve_cell`** — Build new cells instead of clone-and-mutate, avoiding `RawContent::clone_impl()` overhead
3. **Stroke deduplication via thread-local cache** — Identical strokes (common in tables) are computed once and shared via `Arc`
4. **Periodic comemo cache eviction during grid layout** — Frees completed page caches to bound RSS
5. **DiskPageStore streaming for large documents** — Pages are serialized to disk after a small threshold, keeping only recent pages in memory
6. **Streaming PDF finish in krilla (fork)** — PDF is emitted directly to a writer instead of a full in-memory buffer
7. **Flat tag tree + consuming tag serialization** — Tag tree is flattened before resolve and consumed during serialization, avoiding a second full copy during finish
8. **Chunked parallel layout for multi-table docs** — Parallelism is capped per chunk (default 2 concurrent tables) with comemo eviction between chunks, so peak heap tracks in-flight work instead of the whole document

### Speed improvements
9. **Adaptive `SetProcessWorkingSetSize` (Windows)** — At major boundaries (post-layout, post-page-conversion, between chunks) the binary now chooses between `HeapCompact`-only (cheap, no page-fault cost) and full WS trim (expensive but releases RSS) based on `cumulative_grid_entries()`. Small/medium docs (< 200 K entries) skip the expensive trim; only large documents (≥ 200 K) pay it where it prevents swap.
10. **Tuned streaming eviction interval** — Large single-table streaming evicts comemo caches every 5 pages (cheap `HeapCompact`) and does a full WS trim only every 25 pages, avoiding dozens of seconds of trim overhead on 100K-row documents.
11. **Simplified per-cellgrid eviction** — Multi-table documents with many small tables no longer call `HeapCompact` per-table; eviction happens only when cumulative grid entries exceed 15 K, saving thousands of small compaction calls.

All optimizations preserve **byte-identical PDF output** for Simple/SingleAdvanced/MultiTable templates (verified by `tests/correctness_test.py` which compares PDFs from both binaries via PyMuPDF pixel + text comparison).

The Stress template (complex per-department tables with conic gradients) shows a ~0.8 % pixel-level difference concentrated in header/badge decorations — this is because our fork picks up the post-0.14.2 upstream fix `ed96be01b "Make conic gradient rotation clockwise"`. Text, layout, page count and structure remain identical.

## Methodology

### Measurement
- **Peak RAM**: Measured via `psutil.Process.memory_info().rss` polled every 20ms in a separate thread, including child processes
- **Time**: Wall-clock time from `time.time()` around the full compile command
- **PDF size**: `os.path.getsize()` on the output PDF after compilation

### Environment
- **OS**: Windows 11 Pro (10.0.26100)
- **CPU**: Intel Core i9-14900K (32 threads)
- **RAM**: 128 GB DDR5
- **Storage**: NVMe SSD
- **Python**: 3.12.6 with psutil

### Binaries
- **Original**: Typst 0.14.2 official release (`typst-x86_64-pc-windows-msvc`)
- **Optimized**: Built from this fork (`cargo build --release`)

### Reproducibility

All benchmark infrastructure is included in the `benchmarks/` directory:

```bash
# 1. Generate test data (100 rows to 1.2M rows, both formats)
python benchmarks/generate_benchmark_data.py

# 2. Run benchmarks (adjust flags as needed)
python benchmarks/run_benchmarks.py                     # Full suite
python benchmarks/run_benchmarks.py --quick             # Up to 100K only
python benchmarks/run_benchmarks.py --opt-only           # Optimized binary only
python benchmarks/run_benchmarks.py --sizes 100 10000   # Specific sizes

# 3. Generate graphs
python benchmarks/plot_benchmarks.py benchmarks/benchmark_results.json --output-dir benchmarks/

# 4. Merge result files (if running in batches)
python benchmarks/merge_results.py file1.json file2.json merged.json
```

Requirements: `pip install psutil matplotlib numpy`

### Data Sizes

| Rows | Simple JSON | Advanced JSON |
|------|-------------|---------------|
| 100 | 24 KB | 36 KB |
| 1,000 | 246 KB | 365 KB |
| 10,000 | 2.4 MB | 3.5 MB |
| 50,000 | 12 MB | 17.6 MB |
| 100,000 | 24 MB | 35.2 MB |
| 300,000 | 73 MB | 105.9 MB |
| 600,000 | 146 MB | 212 MB |
| 1,200,000 | 293 MB | 425 MB |

## Raw Data

Full benchmark results with all metadata are in [`benchmarks/benchmark_results.json`](benchmarks/benchmark_results.json).
