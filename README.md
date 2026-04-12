<h1 align="center">
  <img alt="Typst" src="https://user-images.githubusercontent.com/17899797/226108480-722b770e-6313-40d7-84f2-26bebb55a281.png">
</h1>

<h3 align="center">Memory-Optimized Fork &mdash; 75-85% Less RAM for Large Tables</h3>

<p align="center">
  <a href="#benchmark-results">
    <img alt="RAM Reduction" src="https://img.shields.io/badge/RAM_reduction-up_to_85%25-2E7D32?style=for-the-badge">
  </a>
  <a href="#benchmark-results">
    <img alt="Speedup" src="https://img.shields.io/badge/speedup-up_to_2.6x-1565C0?style=for-the-badge">
  </a>
  <a href="BENCHMARKS.md">
    <img alt="Benchmarks" src="https://img.shields.io/badge/full_benchmarks-view_report-E65100?style=for-the-badge">
  </a>
</p>

---

> **This is a fork of [typst/typst](https://github.com/typst/typst) v0.14.2** with targeted memory optimizations for large-document compilation. All 3,376 upstream tests pass and PDF output is byte-identical to the original binary. These changes are proposed for upstream integration.

## The Problem

When compiling documents with large tables (10K+ rows), Typst's memory usage grows disproportionately — a 100K-row table document that produces a ~250 MB PDF consumed **16 GB of RAM** with the original binary. This made Typst impractical for production PDF generation from database exports, reports, and other data-heavy workflows.

## The Solution

Through systematic heap profiling with [dhat](https://docs.rs/dhat/latest/dhat/) and analysis of Typst's layout pipeline, I identified several root causes of excessive memory allocation:

1. **Deep cloning in `Content::set()`** — Every table cell triggered `make_unique()` deep copies when setting location metadata. Moved `Location` from `Content` to `Tag` to eliminate these clones entirely.

2. **Per-cell `Packed<TableCell>` allocation** — `resolve_cell` cloned and mutated cells, triggering `RawContent::clone_impl()`. Switched to direct cell construction without clone-and-mutate.

3. **Duplicate stroke computation** — Identical table strokes were recomputed per cell. Added thread-local `Arc`-based stroke deduplication cache.

4. **Unbounded comemo cache growth** — The memoization cache grew without bound during grid layout. Added periodic eviction every 15 finished pages.

5. **All pages held in memory during PDF export** — Added `DiskPageStore` streaming: pages are serialized to disk after runs of >100 pages, keeping only recent pages in memory.

All optimizations preserve **byte-identical PDF output** — verified by automated comparison against the original binary (see `tests/correctness_test.py`).

## Benchmark Results

Tested on Windows 11, Intel Core i9-14900K (32 threads), 128 GB DDR5. Three table templates of increasing complexity, from 100 to 1.2M rows.

### At 100,000 Rows (largest size both binaries handle)

<p align="center">
  <img alt="Summary" src="benchmarks/summary.png" width="800">
</p>

| Template | Original RAM | Optimized RAM | Reduction | Original Time | Optimized Time | Speedup |
|----------|-------------|--------------|-----------|--------------|---------------|---------|
| Simple Table | 16.1 GB | 2.4 GB | **85%** | 41.8s | 16.2s | **2.6x** |
| Single Table (Advanced) | 15.5 GB | 3.4 GB | **78%** | 44.8s | 21.9s | **2.0x** |
| Multi-Table (Advanced) | 14.7 GB | 3.7 GB | **75%** | 36.4s | 27.8s | **1.3x** |

### Memory Scaling

<p align="center">
  <img alt="Memory Comparison" src="benchmarks/memory_comparison.png" width="800">
</p>

### The Optimized Binary Scales Further

The optimized binary successfully compiles documents the original cannot handle at all:

| Rows | Simple Table | Single Table (Adv.) | Multi-Table |
|------|-------------|-------------------|-------------|
| 300K | 6.8 GB / 54s | 10.1 GB / 69s | 10.9 GB / 116s |
| 600K | 13.6 GB / 126s | 20.2 GB / 172s | 21.6 GB / 540s |
| 1.2M | 27.6 GB / 7m | 40.5 GB / 11m | *(exceeds practical limits)* |

<p align="center">
  <a href="BENCHMARKS.md"><strong>View full benchmark report with all graphs and methodology &rarr;</strong></a>
</p>

## Reproducing the Benchmarks

All benchmark infrastructure is included. Anyone can reproduce these results:

```bash
pip install psutil matplotlib numpy

# Generate test data (100 rows to 1.2M rows)
python benchmarks/generate_benchmark_data.py

# Run benchmarks
python benchmarks/run_benchmarks.py --quick      # Up to 100K rows
python benchmarks/run_benchmarks.py               # Full suite

# Generate graphs
python benchmarks/plot_benchmarks.py
```

## Building from Source

```bash
git clone https://github.com/gpradofe/typst.git
cd typst
cargo build --release
```

The optimized binary will be at `target/release/typst` (or `typst.exe` on Windows).

## Running Tests

```bash
# All 3,376 tests must pass
cargo test --release -p typst-tests
```

## About This Work

This research was conducted by **[Gustavo Prado](https://github.com/gpradofe)**, who identified the memory scaling issues in Typst while using it for production PDF generation at work. After discovering that large table documents consumed disproportionate amounts of RAM, Gustavo systematically profiled the Typst compiler using dhat heap profiling, traced the root causes through the layout and PDF export pipeline, and designed the optimization strategy.

AI assistance (Claude) was used to accelerate the implementation of the optimizations and automate benchmark infrastructure, but the problem identification, profiling analysis, architectural decisions, and validation were led by Gustavo.

The goal is to contribute these optimizations upstream to the [Typst project](https://github.com/typst/typst) to benefit all users working with large documents.

## Files Changed

40+ files across 5 crates (`typst-library`, `typst-layout`, `typst-pdf`, `typst`, `typst-cli`). Key modifications:

- **`typst-library`** — `Content`/`Tag` restructuring, direct cell construction, stroke cache, engine flags
- **`typst-layout`** — Periodic comemo eviction, memoize gating, `DiskPageStore` streaming, page spilling
- **`typst-pdf`** — Flat tag tree, streaming PDF conversion
- **`typst-cli`** — Streaming PDF export for large documents

For the complete list, see the [CLAUDE.md](CLAUDE.md) file.

---

*Below is the original Typst README.*

---

## What is Typst?

Typst is a new markup-based typesetting system that is designed to be as powerful
as LaTeX while being much easier to learn and use. Typst has:

- Built-in markup for the most common formatting tasks
- Flexible functions for everything else
- A tightly integrated scripting system
- Math typesetting, bibliography management, and more
- Fast compile times thanks to incremental compilation
- Friendly error messages in case something goes wrong

This repository contains the Typst compiler and its CLI, which is everything you
need to compile Typst documents locally. For the best writing experience,
consider signing up to our [collaborative online editor][app] for free.

## Example
A [gentle introduction][tutorial] to Typst is available in our documentation.
However, if you want to see the power of Typst encapsulated in one image, here
it is:
<p align="center">
 <img alt="Example" width="900" src="https://user-images.githubusercontent.com/17899797/228031796-ced0e452-fcee-4ae9-92da-b9287764ff25.png">
</p>


Let's dissect what's going on:

- We use _set rules_ to configure element properties like the size of pages or
  the numbering of headings. By setting the page height to `auto`, it scales to
  fit the content. Set rules accommodate the most common configurations. If you
  need full control, you can also use [show rules][show] to completely redefine
  the appearance of an element.

- We insert a heading with the `= Heading` syntax. One equals sign creates a top
  level heading, two create a subheading and so on. Typst has more lightweight
  markup like this; see the [syntax] reference for a full list.

- [Mathematical equations][math] are enclosed in dollar signs. By adding extra
  spaces around the contents of an equation, we can put it into a separate block.
  Multi-letter identifiers are interpreted as Typst definitions and functions
  unless put into quotes. This way, we don't need backslashes for things like
  `floor` and `sqrt`. And `phi.alt` applies the `alt` modifier to the `phi` to
  select a particular symbol variant.

- Now, we get to some [scripting]. To input code into a Typst document, we can
  write a hash followed by an expression. We define two variables and a
  recursive function to compute the n-th fibonacci number. Then, we display the
  results in a center-aligned table. The table function takes its cells
  row-by-row. Therefore, we first pass the formulas `$F_1$` to `$F_8$` and then
  the computed fibonacci numbers. We apply the spreading operator (`..`) to both
  because they are arrays and we want to pass the arrays' items as individual
  arguments.

<details>
  <summary>Text version of the code example.</summary>

  ```typst
  #set page(width: 10cm, height: auto)
  #set heading(numbering: "1.")

  = Fibonacci sequence
  The Fibonacci sequence is defined through the
  recurrence relation $F_n = F_(n-1) + F_(n-2)$.
  It can also be expressed in _closed form:_

  $ F_n = round(1 / sqrt(5) phi.alt^n), quad
    phi.alt = (1 + sqrt(5)) / 2 $

  #let count = 8
  #let nums = range(1, count + 1)
  #let fib(n) = (
    if n <= 2 { 1 }
    else { fib(n - 1) + fib(n - 2) }
  )

  The first #count numbers of the sequence are:

  #align(center, table(
    columns: count,
    ..nums.map(n => $F_#n$),
    ..nums.map(n => str(fib(n))),
  ))
  ```
</details>

## Installation
Typst's CLI is available from different sources:

- You can get sources and pre-built binaries for the latest release of Typst
  from the [releases page][releases]. Download the archive for your platform and
  place it in a directory that is in your `PATH`. To stay up to date with future
  releases, you can simply run `typst update`.

- You can install Typst through different package managers. Note that the
  versions in the package managers might lag behind the latest release.
  - Linux:
      - View [Typst on Repology][repology]
      - View [Typst's Snap][snap]
  - macOS: `brew install typst`
  - Windows: `winget install --id Typst.Typst`

- If you have a [Rust][rust] toolchain installed, you can install
  - the latest released Typst version with
    `cargo install --locked typst-cli`
  - a development version with
    `cargo install --git https://github.com/typst/typst --locked typst-cli`

- Nix users can
  - use the `typst` package with `nix-shell -p typst`
  - build and run the [Typst flake](https://github.com/typst/typst-flake) with
    `nix run github:typst/typst-flake -- --version`.

- Docker users can run a prebuilt image with
  `docker run ghcr.io/typst/typst:latest --help`.

## Usage
Once you have installed Typst, you can use it like this:
```sh
# Creates `file.pdf` in working directory.
typst compile file.typ

# Creates a PDF file at the desired path.
typst compile path/to/source.typ path/to/output.pdf
```

You can also watch source files and automatically recompile on changes. This is
faster than compiling from scratch each time because Typst has incremental
compilation.
```sh
# Watches source files and recompiles on changes.
typst watch file.typ
```

Typst further allows you to add custom font paths for your project and list all
of the fonts it discovered:
```sh
# Adds additional directories to search for fonts.
typst compile --font-path path/to/fonts file.typ

# Lists all of the discovered fonts in the system and the given directory.
typst fonts --font-path path/to/fonts

# Or via environment variable (Linux syntax).
TYPST_FONT_PATHS=path/to/fonts typst fonts
```

For other CLI subcommands and options, see below:
```sh
# Prints available subcommands and options.
typst help

# Prints detailed usage of a subcommand.
typst help watch
```

If you prefer an integrated IDE-like experience with autocompletion and instant 
preview, you can also check out our [free web app][app]. Alternatively, there is 
a community-created language server called 
[Tinymist](https://myriad-dreamin.github.io/tinymist/) which is integrated into 
various editor extensions.

## Community
The main places where the community gathers are our [Forum][forum] and our
[Discord server][discord]. The Forum is a great place to ask questions, help
others, and share cool things you created with Typst. The Discord server is more
suitable for quicker questions, discussions about contributing, or just to chat.
We'd be happy to see you there!

[Typst Universe][universe] is where the community shares templates and packages.
If you want to share your own creations, you can submit them to our
[package repository][packages].

If you had a bad experience in our community, please [reach out to us][contact].

## Contributing
We love to see contributions from the community. If you experience bugs, feel
free to open an issue. If you would like to implement a new feature or bug fix,
please follow the steps outlined in the [contribution guide][contributing].

To build Typst yourself, first ensure that you have the
[latest stable Rust][rust] installed. Then, clone this repository and build the
CLI with the following commands:

```sh
git clone https://github.com/typst/typst
cd typst
cargo build --release
```

The optimized binary will be stored in `target/release/`.

Another good way to contribute is by [sharing packages][packages] with the
community.

## Pronunciation and Spelling
IPA: /taɪpst/. "Ty" like in **Ty**pesetting and "pst" like in Hi**pst**er. When
writing about Typst, capitalize its name as a proper noun, with a capital "T".

## Design Principles
All of Typst has been designed with three key goals in mind: Power,
simplicity, and performance. We think it's time for a system that matches the
power of LaTeX, is easy to learn and use, all while being fast enough to realize
instant preview. To achieve these goals, we follow three core design principles:

- **Simplicity through Consistency:**
  If you know how to do one thing in Typst, you should be able to transfer that
  knowledge to other things. If there are multiple ways to do the same thing,
  one of them should be at a different level of abstraction than the other. E.g.
  it's okay that `= Introduction` and `#heading[Introduction]` do the same thing
  because the former is just syntax sugar for the latter.

- **Power through Composability:**
  There are two ways to make something flexible: Have a knob for everything or
  have a few knobs that you can combine in many ways. Typst is designed with the
  second way in mind. We provide systems that you can compose in ways we've
  never even thought of. TeX is also in the second category, but it's a bit
  low-level and therefore people use LaTeX instead. But there, we don't really
  have that much composability. Instead, there's a package for everything
  (`\usepackage{knob}`).

- **Performance through Incrementality:**
  All Typst language features must accommodate for incremental compilation.
  Luckily we have [`comemo`], a system for incremental compilation which does
  most of the hard work in the background.

## Acknowledgements

We'd like to thank everyone who is supporting Typst's development, be it via
[GitHub sponsors] or elsewhere. In particular, special thanks[^1] go to:

- [Posit](https://posit.co/blog/posit-and-typst/) for financing a full-time
  compiler engineer
- [NLnet](https://nlnet.nl/) for supporting work on Typst via multiple grants
  through the [NGI Zero Core](https://nlnet.nl/core) fund:
  - Work on [HTML export](https://nlnet.nl/project/Typst-HTML/)
  - Work on [PDF accessibility](https://nlnet.nl/project/Typst-Accessibility/)
- [Science & Startups](https://www.science-startups.berlin/) for having financed
  Typst development from January through June 2023 via the Berlin Startup
  Scholarship
- [Zerodha](https://zerodha.tech/blog/1-5-million-pdfs-in-25-minutes/) for their
  generous one-time sponsorship

[^1]: This list only includes contributions for our open-source work that exceed
    or are expected to exceed €10K.

[docs]: https://typst.app/docs/
[app]: https://typst.app/
[discord]: https://discord.gg/2uDybryKPe
[forum]: https://forum.typst.app/
[universe]: https://typst.app/universe/
[tutorial]: https://typst.app/docs/tutorial/
[show]: https://typst.app/docs/reference/styling/#show-rules
[math]: https://typst.app/docs/reference/math/
[syntax]: https://typst.app/docs/reference/syntax/
[scripting]: https://typst.app/docs/reference/scripting/
[rust]: https://rustup.rs/
[releases]: https://github.com/typst/typst/releases/
[repology]: https://repology.org/project/typst/versions
[contact]: https://typst.app/contact
[architecture]: https://github.com/typst/typst/blob/main/docs/dev/architecture.md
[contributing]: https://github.com/typst/typst/blob/main/CONTRIBUTING.md
[packages]: https://github.com/typst/packages/
[`comemo`]: https://github.com/typst/comemo/
[snap]: https://snapcraft.io/typst
[GitHub sponsors]: https://github.com/sponsors/typst/
