# Typst memory-optimization benchmark suite

Reproduces the headline numbers in [`../BENCHMARKS.md`](../BENCHMARKS.md): peak RSS and wall-clock time for the optimized fork versus the original Typst 0.14.2 binary across three table-heavy templates and several document sizes.

## One-line run

```
pip install -r requirements.txt
python run_benchmarks.py --quick
```

The runner takes care of everything: it builds the optimized binary (`cargo build --release`), downloads the original 0.14.2 release binary into `bin/original/`, generates any missing JSON test data into `data/`, then runs the matrix.

> Requires Python 3.9+ and a Rust toolchain (1.92.0+, see `../rust-toolchain.toml` if present). `--quick` covers sizes 100 → 100K and takes about 10 minutes on a modern desktop. `python run_benchmarks.py` (no flag) covers up to 1.2 M rows and takes 1–3 hours.

## Flags

| Flag | Purpose |
|---|---|
| `--quick` | Sizes 100 → 100K only (default is 100 → 1.2M) |
| `--sizes 100 1000 10000` | Custom size list |
| `--templates simple multi-table` | Subset of templates (see below) |
| `--opt-only` / `--orig-only` | Skip the other binary |
| `--runs N` | Repeat each config N times (for averaging) |
| `--no-build` | Don't try to `cargo build --release` if the optimized binary is missing |
| `--no-download` | Don't try to fetch the 0.14.2 release binary |
| `--no-generate` | Don't try to create missing data files |
| `--output path.json` | Where to dump raw results (default: `benchmark_results.json`) |

Templates: `simple`, `single-table-advanced`, `multi-table`. See `templates/` for the actual Typst sources and [`../BENCHMARKS.md`](../BENCHMARKS.md) for what each one is meant to exercise.

## Output

- Console: a `bname / template @ rows` line per test plus a summary table at the end.
- `benchmark_results.json`: machine-readable raw data including system info, PDF size, peak RSS in bytes, and wall-clock seconds per (binary × template × size × run).

## Environment overrides

| Variable | Default | Use when |
|---|---|---|
| `TYPST_BIN` | `bin/original/typst[.exe]` | You already have a 0.14.2 binary elsewhere |
| `TYPST_OPT` | `../target/release/typst[.exe]` | You've built the optimized binary in a custom target dir |
| `TYPST_DATA_DIR` | `data/` | You want the generated JSON to live elsewhere (e.g. a fast SSD scratch dir) |

## What gets measured

- **Peak RSS** — `psutil.Process.memory_info().rss` polled every 20 ms across the typst process and all children. This is OS-visible peak resident set size, not dhat-reported bytes-at-peak.
- **Wall-clock time** — around the full `typst compile` invocation, including PDF write.
- **PDF size** — `os.path.getsize()` after compile.

## Reproducing the headline run

```
python run_benchmarks.py --sizes 100000 --runs 1
```

Expected result on a modern x86-64 desktop (results vary with allocator and disk speed):

| Template | Original RAM | Optimized RAM | Reduction |
|---|---:|---:|---:|
| `simple` | ~16 GB | ~450 MB | ~97 % |
| `single-table-advanced` | ~15 GB | ~560 MB | ~96 % |
| `multi-table` | ~15 GB | ~690 MB | ~95 % |

> The original binary requires ~16 GB free RAM at 100K rows and ~90 GB at 600K rows. The optimized fork stays under ~3.5 GB at 600K and finishes a 1.2 M-row workload (>5 minutes) that the original cannot run on a 128 GB workstation.

## File layout

```
benchmarks/
├── README.md                       (this file)
├── requirements.txt                psutil for the runner
├── run_benchmarks.py               main entry point (handles build/download/data/run)
├── generate_benchmark_data.py      JSON test-data generator (called automatically)
├── templates/
│   ├── table_test.typ              "simple"
│   ├── single_table_advanced_test.typ
│   ├── advanced_table_test.typ     "multi-table"
│   └── stress_test.typ             exercises every table feature (not in the default matrix)
├── data/                           generated JSON test data (.gitignored)
└── bin/original/                   downloaded 0.14.2 release binary (.gitignored)
```
