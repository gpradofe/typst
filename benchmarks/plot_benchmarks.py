"""
Generate publication-quality benchmark graphs from benchmark_results.json.

Produces:
  - summary.png               — Hero overview graph
  - memory_comparison.png     — Peak RAM: original vs optimized (log scale)
  - time_comparison.png       — Compilation time: original vs optimized (log scale)
  - memory_reduction.png      — % RAM reduction by template
  - speedup.png               — Speedup factor by template
  - scaling.png               — RAM per row scaling curve
  - optimized_scaling.png     — Absolute scaling for large sizes (opt only)
  - bar_comparison.png        — Side-by-side bars at key sizes

Usage:
    python plot_benchmarks.py                          # Use benchmark_results.json
    python plot_benchmarks.py path/to/results.json     # Custom results file
    python plot_benchmarks.py --output-dir ./graphs     # Custom output directory
"""
import json
import os
import sys
import argparse

try:
    import matplotlib
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    import matplotlib.ticker as ticker
    import matplotlib.patheffects as pe
    from matplotlib.lines import Line2D
except ImportError:
    print("ERROR: matplotlib required. Install with: pip install matplotlib")
    sys.exit(1)

try:
    import numpy as np
except ImportError:
    print("ERROR: numpy required. Install with: pip install numpy")
    sys.exit(1)


# ── Professional Style ────────────────────────────────────────────────────

# Color palette — carefully chosen for contrast and colorblind friendliness
COLORS = {
    "original":              "#D32F2F",   # Material Red 700
    "optimized":             "#2E7D32",   # Material Green 800
    "simple":                "#1565C0",   # Material Blue 800
    "single-table-advanced": "#E65100",   # Material Orange 900
    "multi-table":           "#6A1B9A",   # Material Purple 800
}

COLORS_LIGHT = {
    "simple":                "#90CAF9",   # Blue 200
    "single-table-advanced": "#FFCC80",   # Orange 200
    "multi-table":           "#CE93D8",   # Purple 200
}

TEMPLATE_LABELS = {
    "simple": "Simple Table",
    "single-table-advanced": "Single Table (Advanced)",
    "multi-table": "Multi-Table (Advanced)",
}

TEMPLATE_SHORT = {
    "simple": "Simple",
    "single-table-advanced": "Single Adv.",
    "multi-table": "Multi-Table",
}

MARKERS = {
    "simple": "o",
    "single-table-advanced": "s",
    "multi-table": "D",
}

# Global matplotlib style
plt.rcParams.update({
    "figure.facecolor": "white",
    "axes.facecolor": "#FAFAFA",
    "axes.edgecolor": "#CCCCCC",
    "axes.grid": True,
    "axes.axisbelow": True,
    "grid.color": "#E0E0E0",
    "grid.linewidth": 0.6,
    "grid.alpha": 0.7,
    "font.family": "sans-serif",
    "font.sans-serif": ["Segoe UI", "Helvetica Neue", "Arial", "DejaVu Sans"],
    "font.size": 11,
    "axes.titlesize": 16,
    "axes.titleweight": "bold",
    "axes.labelsize": 13,
    "axes.labelweight": "medium",
    "legend.fontsize": 10,
    "legend.framealpha": 0.95,
    "legend.edgecolor": "#CCCCCC",
    "figure.dpi": 150,
    "savefig.dpi": 150,
    "savefig.bbox": "tight",
    "savefig.pad_inches": 0.15,
    "xtick.labelsize": 10,
    "ytick.labelsize": 10,
    "xtick.color": "#555555",
    "ytick.color": "#555555",
    "axes.labelcolor": "#333333",
    "axes.titlecolor": "#222222",
})


def format_rows(n):
    if n >= 1_000_000:
        return f"{n/1_000_000:.1f}M"
    elif n >= 1_000:
        return f"{n//1_000}K"
    return str(n)


def format_mb(mb):
    if mb >= 1000:
        return f"{mb/1000:.1f} GB"
    return f"{mb:.0f} MB"


def load_results(path):
    with open(path) as f:
        data = json.load(f)
    results = [r for r in data["results"] if not r.get("skipped") and r.get("ok")]
    return data, results


def add_subtitle(ax, text, y=1.02):
    """Add a lighter subtitle below the title."""
    ax.text(0.5, y, text, transform=ax.transAxes, ha="center", va="bottom",
            fontsize=10, color="#666666", style="italic")


def annotate_point(ax, x, y, text, color, offset=(8, 6), fontsize=9, bold=True):
    """Annotate a data point with a label and white outline for readability."""
    weight = "bold" if bold else "normal"
    ax.annotate(
        text, (x, y),
        textcoords="offset points", xytext=offset,
        fontsize=fontsize, fontweight=weight, color=color,
        path_effects=[pe.withStroke(linewidth=3, foreground="white")]
    )


# ── Graph 1: Memory Comparison (log-log) ──────────────────────────────────

def plot_memory_comparison(results, output_dir):
    fig, ax = plt.subplots(figsize=(13, 7.5))

    templates = sorted(set(r["template"] for r in results))

    for tname in templates:
        for binary in ["original", "optimized"]:
            pts = sorted(
                [r for r in results if r["template"] == tname and r["binary"] == binary],
                key=lambda r: r["rows"]
            )
            if not pts:
                continue
            rows = [p["rows"] for p in pts]
            ram = [p["peak_ram_mb"] for p in pts]
            style = "-" if binary == "optimized" else "--"
            marker = MARKERS.get(tname, "o")
            color = COLORS.get(tname, "#333")
            alpha = 1.0 if binary == "optimized" else 0.5
            lw = 2.5 if binary == "optimized" else 1.8
            ms = 8 if binary == "optimized" else 6
            label = f"{TEMPLATE_LABELS.get(tname, tname)} ({binary})"
            ax.plot(rows, ram, style, marker=marker, color=color, alpha=alpha,
                    markersize=ms, linewidth=lw, label=label, zorder=3)

            # Annotate the last point for optimized
            if binary == "optimized" and ram:
                annotate_point(ax, rows[-1], ram[-1], format_mb(ram[-1]),
                              color, offset=(10, -5))

    ax.set_xscale("log")
    ax.set_yscale("log")
    ax.set_xlabel("Number of Data Rows")
    ax.set_ylabel("Peak Memory (MB)")
    ax.set_title("Peak Memory Usage: Original vs Optimized")
    add_subtitle(ax, "Dashed = original Typst 0.14.2  |  Solid = optimized fork")

    ax.xaxis.set_major_formatter(ticker.FuncFormatter(lambda x, _: format_rows(int(x))))
    ax.yaxis.set_major_formatter(ticker.FuncFormatter(lambda x, _: f"{x:,.0f}"))

    # Reference lines for memory thresholds
    for threshold, label_text in [(1024, "1 GB"), (4096, "4 GB"), (16384, "16 GB")]:
        ax.axhline(y=threshold, color="#BDBDBD", linestyle=":", linewidth=0.8)
        ax.text(80, threshold * 1.12, label_text, fontsize=8, color="#999",
                ha="left", va="bottom")

    ax.legend(loc="upper left", framealpha=0.95, borderpad=0.8)
    fig.tight_layout(pad=1.5)
    path = os.path.join(output_dir, "memory_comparison.png")
    fig.savefig(path)
    plt.close(fig)
    print(f"  Saved {path}")


# ── Graph 2: Time Comparison (log-log) ────────────────────────────────────

def plot_time_comparison(results, output_dir):
    fig, ax = plt.subplots(figsize=(13, 7.5))

    templates = sorted(set(r["template"] for r in results))

    for tname in templates:
        for binary in ["original", "optimized"]:
            pts = sorted(
                [r for r in results if r["template"] == tname and r["binary"] == binary],
                key=lambda r: r["rows"]
            )
            if not pts:
                continue
            rows = [p["rows"] for p in pts]
            times = [p["time_s"] for p in pts]
            style = "-" if binary == "optimized" else "--"
            marker = MARKERS.get(tname, "o")
            color = COLORS.get(tname, "#333")
            alpha = 1.0 if binary == "optimized" else 0.5
            lw = 2.5 if binary == "optimized" else 1.8
            ms = 8 if binary == "optimized" else 6
            label = f"{TEMPLATE_LABELS.get(tname, tname)} ({binary})"
            ax.plot(rows, times, style, marker=marker, color=color, alpha=alpha,
                    markersize=ms, linewidth=lw, label=label, zorder=3)

            # Annotate last optimized point
            if binary == "optimized" and times:
                t = times[-1]
                lbl = f"{t/60:.0f}m {t%60:.0f}s" if t >= 60 else f"{t:.1f}s"
                annotate_point(ax, rows[-1], times[-1], lbl, color, offset=(10, -5))

    ax.set_xscale("log")
    ax.set_yscale("log")
    ax.set_xlabel("Number of Data Rows")
    ax.set_ylabel("Compilation Time (seconds)")
    ax.set_title("Compilation Time: Original vs Optimized")
    add_subtitle(ax, "Dashed = original Typst 0.14.2  |  Solid = optimized fork")

    ax.xaxis.set_major_formatter(ticker.FuncFormatter(lambda x, _: format_rows(int(x))))

    ax.legend(loc="upper left", framealpha=0.95, borderpad=0.8)
    fig.tight_layout(pad=1.5)
    path = os.path.join(output_dir, "time_comparison.png")
    fig.savefig(path)
    plt.close(fig)
    print(f"  Saved {path}")


# ── Graph 3: RAM Reduction % ─────────────────────────────────────────────

def plot_memory_reduction(results, output_dir):
    fig, ax = plt.subplots(figsize=(12, 6.5))

    templates = sorted(set(r["template"] for r in results))

    for tname in templates:
        orig_pts = {r["rows"]: r for r in results
                    if r["template"] == tname and r["binary"] == "original"}
        opt_pts = {r["rows"]: r for r in results
                   if r["template"] == tname and r["binary"] == "optimized"}
        common = sorted(set(orig_pts.keys()) & set(opt_pts.keys()))
        if not common:
            continue

        rows = common
        reductions = [(1 - opt_pts[s]["peak_ram_mb"] / orig_pts[s]["peak_ram_mb"]) * 100
                      for s in rows]

        marker = MARKERS.get(tname, "o")
        color = COLORS.get(tname, "#333")
        label = TEMPLATE_LABELS.get(tname, tname)
        ax.plot(rows, reductions, "-", marker=marker, color=color,
                markersize=9, linewidth=2.8, label=label, zorder=3)

        # Annotate the last (largest) point
        annotate_point(ax, rows[-1], reductions[-1], f"{reductions[-1]:.0f}%",
                      color, offset=(12, 0), fontsize=12)

    ax.set_xscale("log")
    ax.set_xlabel("Number of Data Rows")
    ax.set_ylabel("RAM Reduction (%)")
    ax.set_title("Memory Savings: Optimized vs Original")
    add_subtitle(ax, "Higher is better \u2014 percentage of RAM saved by the optimized binary")
    ax.set_ylim(0, 100)

    # Shade the "good" zone
    ax.axhspan(70, 100, color="#C8E6C9", alpha=0.3, zorder=0)
    ax.axhspan(50, 70, color="#FFF9C4", alpha=0.3, zorder=0)
    ax.text(80, 92, "Excellent (>70%)", fontsize=8, color="#388E3C", alpha=0.7, ha="left")
    ax.text(80, 55, "Good (50-70%)", fontsize=8, color="#F9A825", alpha=0.7, ha="left")

    ax.xaxis.set_major_formatter(ticker.FuncFormatter(lambda x, _: format_rows(int(x))))

    ax.legend(loc="lower right", framealpha=0.95, borderpad=0.8)
    fig.tight_layout(pad=1.5)
    path = os.path.join(output_dir, "memory_reduction.png")
    fig.savefig(path)
    plt.close(fig)
    print(f"  Saved {path}")


# ── Graph 4: Speedup Factor ──────────────────────────────────────────────

def plot_speedup(results, output_dir):
    fig, ax = plt.subplots(figsize=(12, 6.5))

    templates = sorted(set(r["template"] for r in results))

    for tname in templates:
        orig_pts = {r["rows"]: r for r in results
                    if r["template"] == tname and r["binary"] == "original"}
        opt_pts = {r["rows"]: r for r in results
                   if r["template"] == tname and r["binary"] == "optimized"}
        common = sorted(set(orig_pts.keys()) & set(opt_pts.keys()))
        if not common:
            continue

        rows = common
        speedups = [orig_pts[s]["time_s"] / opt_pts[s]["time_s"] for s in rows]

        marker = MARKERS.get(tname, "o")
        color = COLORS.get(tname, "#333")
        label = TEMPLATE_LABELS.get(tname, tname)
        ax.plot(rows, speedups, "-", marker=marker, color=color,
                markersize=9, linewidth=2.8, label=label, zorder=3)

        annotate_point(ax, rows[-1], speedups[-1], f"{speedups[-1]:.1f}x",
                      color, offset=(12, 0), fontsize=12)

    ax.set_xscale("log")
    ax.set_xlabel("Number of Data Rows")
    ax.set_ylabel("Speedup (original time / optimized time)")
    ax.set_title("Compilation Speedup")
    add_subtitle(ax, "Higher is better \u2014 how many times faster the optimized binary compiles")

    ax.axhline(y=1, color="#BDBDBD", linestyle="-", linewidth=1)
    ax.axhspan(2, ax.get_ylim()[1] if ax.get_ylim()[1] > 2 else 4,
               color="#C8E6C9", alpha=0.15, zorder=0)
    ax.text(80, 1.05, "baseline (1x)", fontsize=8, color="#999", ha="left")

    ax.xaxis.set_major_formatter(ticker.FuncFormatter(lambda x, _: format_rows(int(x))))

    ax.legend(loc="upper left", framealpha=0.95, borderpad=0.8)
    fig.tight_layout(pad=1.5)
    path = os.path.join(output_dir, "speedup.png")
    fig.savefig(path)
    plt.close(fig)
    print(f"  Saved {path}")


# ── Graph 5: Scaling \u2014 RAM per 1K rows ────────────────────────────────────

def plot_scaling(results, output_dir):
    fig, axes = plt.subplots(1, 2, figsize=(17, 7))

    templates = sorted(set(r["template"] for r in results))

    # Left: RAM per 1K rows
    ax = axes[0]
    for tname in templates:
        for binary in ["original", "optimized"]:
            pts = sorted(
                [r for r in results if r["template"] == tname and r["binary"] == binary],
                key=lambda r: r["rows"]
            )
            pts = [p for p in pts if p["rows"] >= 1000]
            if not pts:
                continue
            rows = [p["rows"] for p in pts]
            per_k = [p["peak_ram_mb"] / (p["rows"] / 1000) for p in pts]
            style = "-" if binary == "optimized" else "--"
            marker = MARKERS.get(tname, "o")
            color = COLORS.get(tname, "#333")
            alpha = 1.0 if binary == "optimized" else 0.5
            lw = 2.2 if binary == "optimized" else 1.5
            label = f"{TEMPLATE_SHORT.get(tname, tname)} ({binary})"
            ax.plot(rows, per_k, style, marker=marker, color=color, alpha=alpha,
                    markersize=6, linewidth=lw, label=label, zorder=3)

    ax.set_xscale("log")
    ax.set_xlabel("Number of Data Rows")
    ax.set_ylabel("MB per 1,000 Rows")
    ax.set_title("Memory Efficiency")
    add_subtitle(ax, "Lower is better \u2014 RAM cost per 1K rows of data")
    ax.xaxis.set_major_formatter(ticker.FuncFormatter(lambda x, _: format_rows(int(x))))
    ax.legend(loc="upper right", fontsize=8, framealpha=0.95, borderpad=0.6)

    # Right: Time per 1K rows
    ax = axes[1]
    for tname in templates:
        for binary in ["original", "optimized"]:
            pts = sorted(
                [r for r in results if r["template"] == tname and r["binary"] == binary],
                key=lambda r: r["rows"]
            )
            pts = [p for p in pts if p["rows"] >= 1000]
            if not pts:
                continue
            rows = [p["rows"] for p in pts]
            per_k = [p["time_s"] / (p["rows"] / 1000) for p in pts]
            style = "-" if binary == "optimized" else "--"
            marker = MARKERS.get(tname, "o")
            color = COLORS.get(tname, "#333")
            alpha = 1.0 if binary == "optimized" else 0.5
            lw = 2.2 if binary == "optimized" else 1.5
            label = f"{TEMPLATE_SHORT.get(tname, tname)} ({binary})"
            ax.plot(rows, per_k, style, marker=marker, color=color, alpha=alpha,
                    markersize=6, linewidth=lw, label=label, zorder=3)

    ax.set_xscale("log")
    ax.set_xlabel("Number of Data Rows")
    ax.set_ylabel("Seconds per 1,000 Rows")
    ax.set_title("Time Efficiency")
    add_subtitle(ax, "Lower is better \u2014 compile time per 1K rows of data")
    ax.xaxis.set_major_formatter(ticker.FuncFormatter(lambda x, _: format_rows(int(x))))
    ax.legend(loc="upper right", fontsize=8, framealpha=0.95, borderpad=0.6)

    fig.tight_layout(pad=1.8)
    path = os.path.join(output_dir, "scaling.png")
    fig.savefig(path)
    plt.close(fig)
    print(f"  Saved {path}")


# ── Graph 6: Optimized-only scaling for large sizes ───────────────────────

def plot_optimized_scaling(results, output_dir):
    """Show how the optimized binary scales to 1.2M rows."""
    opt_results = [r for r in results if r["binary"] == "optimized"]
    if not opt_results:
        return

    fig, axes = plt.subplots(1, 2, figsize=(15, 7))
    templates = sorted(set(r["template"] for r in opt_results))

    # Left: Absolute RAM
    ax = axes[0]
    for tname in templates:
        pts = sorted(
            [r for r in opt_results if r["template"] == tname],
            key=lambda r: r["rows"]
        )
        if not pts:
            continue
        rows = [p["rows"] for p in pts]
        ram = [p["peak_ram_mb"] for p in pts]
        marker = MARKERS.get(tname, "o")
        color = COLORS.get(tname, "#333")
        label = TEMPLATE_LABELS.get(tname, tname)
        ax.plot(rows, ram, "-", marker=marker, color=color,
                markersize=9, linewidth=2.8, label=label, zorder=3)

        if ram:
            annotate_point(ax, rows[-1], ram[-1], format_mb(ram[-1]),
                          color, offset=(-80, 12), fontsize=10)

    ax.set_xscale("log")
    ax.set_xlabel("Number of Data Rows")
    ax.set_ylabel("Peak RAM (MB)")
    ax.set_title("Optimized Binary: Memory Scaling")
    add_subtitle(ax, "How memory grows as document size increases")
    ax.xaxis.set_major_formatter(ticker.FuncFormatter(lambda x, _: format_rows(int(x))))
    ax.yaxis.set_major_formatter(ticker.FuncFormatter(lambda x, _: format_mb(x)))
    ax.legend(loc="upper left", framealpha=0.95, borderpad=0.8)

    # Right: Absolute time
    ax = axes[1]
    for tname in templates:
        pts = sorted(
            [r for r in opt_results if r["template"] == tname],
            key=lambda r: r["rows"]
        )
        if not pts:
            continue
        rows = [p["rows"] for p in pts]
        times = [p["time_s"] for p in pts]
        marker = MARKERS.get(tname, "o")
        color = COLORS.get(tname, "#333")
        label = TEMPLATE_LABELS.get(tname, tname)
        ax.plot(rows, times, "-", marker=marker, color=color,
                markersize=9, linewidth=2.8, label=label, zorder=3)

        if times:
            t = times[-1]
            lbl = f"{t/60:.0f}m {t%60:.0f}s" if t >= 60 else f"{t:.0f}s"
            annotate_point(ax, rows[-1], times[-1], lbl,
                          color, offset=(-70, 12), fontsize=10)

    ax.set_xscale("log")
    ax.set_xlabel("Number of Data Rows")
    ax.set_ylabel("Compilation Time (seconds)")
    ax.set_title("Optimized Binary: Time Scaling")
    add_subtitle(ax, "Compile time from 100 rows to 1.2 million rows")
    ax.xaxis.set_major_formatter(ticker.FuncFormatter(lambda x, _: format_rows(int(x))))
    ax.legend(loc="upper left", framealpha=0.95, borderpad=0.8)

    fig.tight_layout(pad=1.8)
    path = os.path.join(output_dir, "optimized_scaling.png")
    fig.savefig(path)
    plt.close(fig)
    print(f"  Saved {path}")


# ── Graph 7: Bar chart \u2014 head-to-head at key sizes ───────────────────────

def plot_bar_comparison(results, output_dir):
    """Side-by-side bar chart at key comparison sizes."""
    templates = sorted(set(r["template"] for r in results))

    sizes_with_both = set()
    for tname in templates:
        orig_sizes = {r["rows"] for r in results
                      if r["template"] == tname and r["binary"] == "original"}
        opt_sizes = {r["rows"] for r in results
                     if r["template"] == tname and r["binary"] == "optimized"}
        sizes_with_both |= (orig_sizes & opt_sizes)

    if not sizes_with_both:
        return

    sizes = sorted(sizes_with_both)
    # Skip very small sizes for visual clarity
    sizes = [s for s in sizes if s >= 1000]
    if len(sizes) > 5:
        indices = np.linspace(0, len(sizes)-1, 5, dtype=int)
        sizes = [sizes[i] for i in indices]

    fig, axes = plt.subplots(1, 2, figsize=(17, 7.5))

    n_templates = len(templates)
    n_sizes = len(sizes)
    x = np.arange(n_sizes)
    group_width = 0.75
    bar_width = group_width / (n_templates * 2)

    # Memory bars
    ax = axes[0]
    for i, tname in enumerate(templates):
        orig_ram = []
        opt_ram = []
        for s in sizes:
            o = next((r for r in results if r["template"] == tname
                      and r["binary"] == "original" and r["rows"] == s), None)
            p = next((r for r in results if r["template"] == tname
                      and r["binary"] == "optimized" and r["rows"] == s), None)
            orig_ram.append(o["peak_ram_mb"] if o else 0)
            opt_ram.append(p["peak_ram_mb"] if p else 0)

        offset_orig = -group_width/2 + (2*i) * bar_width + bar_width/2
        offset_opt = -group_width/2 + (2*i+1) * bar_width + bar_width/2

        color = COLORS.get(tname, "#333")
        light = COLORS_LIGHT.get(tname, "#CCC")
        ax.bar(x + offset_orig, orig_ram, bar_width * 0.9,
               label=f"{TEMPLATE_SHORT.get(tname, tname)} (original)",
               color=light, edgecolor=color, linewidth=0.8, zorder=3)
        ax.bar(x + offset_opt, opt_ram, bar_width * 0.9,
               label=f"{TEMPLATE_SHORT.get(tname, tname)} (optimized)",
               color=color, edgecolor=color, linewidth=0.8, zorder=3)

    ax.set_xlabel("Number of Data Rows")
    ax.set_ylabel("Peak RAM (MB)")
    ax.set_title("Memory: Original vs Optimized")
    add_subtitle(ax, "Light bars = original  |  Dark bars = optimized")
    ax.set_xticks(x)
    ax.set_xticklabels([format_rows(s) for s in sizes])
    ax.legend(fontsize=8, loc="upper left", ncol=1, framealpha=0.95)
    ax.yaxis.set_major_formatter(ticker.FuncFormatter(lambda x, _: f"{x:,.0f}"))

    # Time bars
    ax = axes[1]
    for i, tname in enumerate(templates):
        orig_time = []
        opt_time = []
        for s in sizes:
            o = next((r for r in results if r["template"] == tname
                      and r["binary"] == "original" and r["rows"] == s), None)
            p = next((r for r in results if r["template"] == tname
                      and r["binary"] == "optimized" and r["rows"] == s), None)
            orig_time.append(o["time_s"] if o else 0)
            opt_time.append(p["time_s"] if p else 0)

        offset_orig = -group_width/2 + (2*i) * bar_width + bar_width/2
        offset_opt = -group_width/2 + (2*i+1) * bar_width + bar_width/2

        color = COLORS.get(tname, "#333")
        light = COLORS_LIGHT.get(tname, "#CCC")
        ax.bar(x + offset_orig, orig_time, bar_width * 0.9,
               label=f"{TEMPLATE_SHORT.get(tname, tname)} (original)",
               color=light, edgecolor=color, linewidth=0.8, zorder=3)
        ax.bar(x + offset_opt, opt_time, bar_width * 0.9,
               label=f"{TEMPLATE_SHORT.get(tname, tname)} (optimized)",
               color=color, edgecolor=color, linewidth=0.8, zorder=3)

    ax.set_xlabel("Number of Data Rows")
    ax.set_ylabel("Compilation Time (seconds)")
    ax.set_title("Time: Original vs Optimized")
    add_subtitle(ax, "Light bars = original  |  Dark bars = optimized")
    ax.set_xticks(x)
    ax.set_xticklabels([format_rows(s) for s in sizes])
    ax.legend(fontsize=8, loc="upper left", ncol=1, framealpha=0.95)

    fig.tight_layout(pad=1.8)
    path = os.path.join(output_dir, "bar_comparison.png")
    fig.savefig(path)
    plt.close(fig)
    print(f"  Saved {path}")


# ── Graph 8: Summary hero graph ──────────────────────────────────────────

def plot_summary(results, output_dir):
    """Hero overview: memory comparison at 100K rows + scaling curve."""
    fig, axes = plt.subplots(1, 2, figsize=(16, 7))

    templates = sorted(set(r["template"] for r in results))

    # Left: Horizontal bar chart at 100K rows (the headline number)
    ax = axes[0]
    target_size = 100000
    bar_data = []
    for tname in templates:
        orig = next((r for r in results if r["template"] == tname
                     and r["binary"] == "original" and r["rows"] == target_size), None)
        opt = next((r for r in results if r["template"] == tname
                    and r["binary"] == "optimized" and r["rows"] == target_size), None)
        if orig and opt:
            bar_data.append((tname, orig["peak_ram_mb"], opt["peak_ram_mb"],
                           orig["time_s"], opt["time_s"]))

    if bar_data:
        y_pos = np.arange(len(bar_data))
        orig_vals = [d[1] for d in bar_data]
        opt_vals = [d[2] for d in bar_data]
        labels = [TEMPLATE_SHORT.get(d[0], d[0]) for d in bar_data]

        ax.barh(y_pos + 0.18, orig_vals, 0.32, color="#EF9A9A",
                edgecolor="#D32F2F", linewidth=0.8, label="Original", zorder=3)
        ax.barh(y_pos - 0.18, opt_vals, 0.32, color="#A5D6A7",
                edgecolor="#2E7D32", linewidth=0.8, label="Optimized", zorder=3)

        # Add value labels on bars
        for i in range(len(bar_data)):
            reduction = (1 - opt_vals[i] / orig_vals[i]) * 100
            ax.text(orig_vals[i] + 200, y_pos[i] + 0.18,
                    format_mb(orig_vals[i]), va="center", fontsize=10, color="#C62828",
                    fontweight="bold",
                    path_effects=[pe.withStroke(linewidth=2, foreground="white")])
            ax.text(opt_vals[i] + 200, y_pos[i] - 0.18,
                    f"{format_mb(opt_vals[i])}  ({reduction:.0f}% less)",
                    va="center", fontsize=10, color="#1B5E20", fontweight="bold",
                    path_effects=[pe.withStroke(linewidth=2, foreground="white")])

        ax.set_yticks(y_pos)
        ax.set_yticklabels(labels, fontsize=12, fontweight="medium")
        ax.set_xlabel("Peak RAM (MB)")
        ax.set_title(f"Memory at {format_rows(target_size)} Rows")
        add_subtitle(ax, "Red = original Typst  |  Green = optimized fork")
        ax.legend(loc="lower right", framealpha=0.95)
        ax.xaxis.set_major_formatter(ticker.FuncFormatter(lambda x, _: format_mb(x)))

    # Right: Optimized scaling curve (the "we scale further" story)
    ax = axes[1]
    opt_results = [r for r in results if r["binary"] == "optimized"]
    for tname in templates:
        pts = sorted(
            [r for r in opt_results if r["template"] == tname],
            key=lambda r: r["rows"]
        )
        if not pts:
            continue
        rows = [p["rows"] for p in pts]
        ram = [p["peak_ram_mb"] / 1024 for p in pts]  # Convert to GB
        marker = MARKERS.get(tname, "o")
        color = COLORS.get(tname, "#333")
        label = TEMPLATE_SHORT.get(tname, tname)
        ax.plot(rows, ram, "-", marker=marker, color=color,
                markersize=8, linewidth=2.5, label=label, zorder=3)

    # Mark the original binary's limit
    ax.axvline(x=100000, color="#D32F2F", linestyle="--", linewidth=1.5, alpha=0.7)
    ylim = ax.get_ylim()
    y_label = ylim[1] * 0.85 if ylim[1] > 1 else 0.8
    ax.text(115000, y_label,
            "Original binary\nlimit (~16 GB)",
            fontsize=9, color="#D32F2F", alpha=0.8, ha="left",
            path_effects=[pe.withStroke(linewidth=2, foreground="white")])

    ax.set_xscale("log")
    ax.set_xlabel("Number of Data Rows")
    ax.set_ylabel("Peak RAM (GB)")
    ax.set_title("Optimized: Scaling Beyond Original")
    add_subtitle(ax, "The optimized binary handles documents the original cannot")
    ax.xaxis.set_major_formatter(ticker.FuncFormatter(lambda x, _: format_rows(int(x))))
    ax.legend(loc="upper left", framealpha=0.95, borderpad=0.8)

    fig.tight_layout(pad=1.8)
    path = os.path.join(output_dir, "summary.png")
    fig.savefig(path)
    plt.close(fig)
    print(f"  Saved {path}")


# ── Main ──────────────────────────────────────────────────────────────────

def main():
    parser = argparse.ArgumentParser(description="Generate benchmark graphs")
    parser.add_argument("results_file", nargs="?",
                        default=os.path.join(os.path.dirname(os.path.abspath(__file__)),
                                             "benchmark_results.json"),
                        help="Path to benchmark_results.json")
    parser.add_argument("--output-dir", default=None,
                        help="Directory for output graphs (default: same as results file)")
    args = parser.parse_args()

    if not os.path.exists(args.results_file):
        print(f"ERROR: Results file not found: {args.results_file}")
        print("Run 'python run_benchmarks.py' first to generate results.")
        sys.exit(1)

    output_dir = args.output_dir or os.path.dirname(args.results_file)
    os.makedirs(output_dir, exist_ok=True)

    data, results = load_results(args.results_file)

    print(f"Loaded {len(results)} benchmark results")
    print(f"System: {data['system']['platform']}")
    print(f"Generating graphs to {output_dir}/")
    print()

    plot_summary(results, output_dir)
    plot_memory_comparison(results, output_dir)
    plot_time_comparison(results, output_dir)
    plot_memory_reduction(results, output_dir)
    plot_speedup(results, output_dir)
    plot_scaling(results, output_dir)
    plot_optimized_scaling(results, output_dir)
    plot_bar_comparison(results, output_dir)

    print(f"\nAll graphs saved to {output_dir}/")


if __name__ == "__main__":
    main()
