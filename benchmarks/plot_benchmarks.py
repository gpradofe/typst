"""
Generate publication-quality benchmark graphs from benchmark_results.json.

Produces:
  - memory_comparison.png    — Peak RAM: original vs optimized (log scale)
  - time_comparison.png      — Compilation time: original vs optimized (log scale)
  - memory_reduction.png     — % RAM reduction by template
  - speedup.png              — Speedup factor by template
  - scaling.png              — RAM per row scaling curve
  - summary_table.png        — Table image of key results

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
    from matplotlib.lines import Line2D
except ImportError:
    print("ERROR: matplotlib required. Install with: pip install matplotlib")
    sys.exit(1)

try:
    import numpy as np
except ImportError:
    print("ERROR: numpy required. Install with: pip install numpy")
    sys.exit(1)


# ── Style ──────────────────────────────────────────────────────────────────

COLORS = {
    "original": "#e74c3c",      # Red
    "optimized": "#2ecc71",     # Green
    "simple": "#3498db",        # Blue
    "single-table-advanced": "#e67e22",  # Orange
    "multi-table": "#9b59b6",   # Purple
}

TEMPLATE_LABELS = {
    "simple": "Simple Table",
    "single-table-advanced": "Single Table (Advanced)",
    "multi-table": "Multi-Table (Advanced)",
}

MARKERS = {
    "simple": "o",
    "single-table-advanced": "s",
    "multi-table": "D",
}

plt.rcParams.update({
    "figure.facecolor": "white",
    "axes.facecolor": "#fafafa",
    "axes.grid": True,
    "grid.alpha": 0.3,
    "font.size": 11,
    "axes.titlesize": 14,
    "axes.labelsize": 12,
    "legend.fontsize": 10,
    "figure.dpi": 150,
})


def format_rows(n):
    if n >= 1_000_000:
        return f"{n/1_000_000:.1f}M"
    elif n >= 1_000:
        return f"{n//1_000}K"
    return str(n)


def load_results(path):
    with open(path) as f:
        data = json.load(f)
    # Filter to successful, non-skipped results
    results = [r for r in data["results"] if not r.get("skipped") and r.get("ok")]
    return data, results


# ── Graph 1: Memory Comparison (log-log) ──────────────────────────────────

def plot_memory_comparison(results, output_dir):
    fig, ax = plt.subplots(figsize=(12, 7))

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
            alpha = 1.0 if binary == "optimized" else 0.6
            label = f"{TEMPLATE_LABELS.get(tname, tname)} ({binary})"
            ax.plot(rows, ram, style, marker=marker, color=color, alpha=alpha,
                    markersize=7, linewidth=2, label=label)

    ax.set_xscale("log")
    ax.set_yscale("log")
    ax.set_xlabel("Number of Rows")
    ax.set_ylabel("Peak RAM (MB)")
    ax.set_title("Peak Memory Usage: Original vs Optimized")

    ax.xaxis.set_major_formatter(ticker.FuncFormatter(lambda x, _: format_rows(int(x))))
    ax.yaxis.set_major_formatter(ticker.FuncFormatter(lambda x, _: f"{x:,.0f}"))

    # Add reference lines
    ax.axhline(y=1024, color="gray", linestyle=":", alpha=0.4, linewidth=1)
    ax.text(ax.get_xlim()[0] * 1.5, 1100, "1 GB", fontsize=9, color="gray", alpha=0.6)
    ax.axhline(y=8192, color="gray", linestyle=":", alpha=0.4, linewidth=1)
    ax.text(ax.get_xlim()[0] * 1.5, 8800, "8 GB", fontsize=9, color="gray", alpha=0.6)

    ax.legend(loc="upper left", framealpha=0.9)
    fig.tight_layout()
    path = os.path.join(output_dir, "memory_comparison.png")
    fig.savefig(path, dpi=150, bbox_inches="tight")
    plt.close(fig)
    print(f"  Saved {path}")


# ── Graph 2: Time Comparison (log-log) ────────────────────────────────────

def plot_time_comparison(results, output_dir):
    fig, ax = plt.subplots(figsize=(12, 7))

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
            alpha = 1.0 if binary == "optimized" else 0.6
            label = f"{TEMPLATE_LABELS.get(tname, tname)} ({binary})"
            ax.plot(rows, times, style, marker=marker, color=color, alpha=alpha,
                    markersize=7, linewidth=2, label=label)

    ax.set_xscale("log")
    ax.set_yscale("log")
    ax.set_xlabel("Number of Rows")
    ax.set_ylabel("Compilation Time (seconds)")
    ax.set_title("Compilation Time: Original vs Optimized")

    ax.xaxis.set_major_formatter(ticker.FuncFormatter(lambda x, _: format_rows(int(x))))

    ax.legend(loc="upper left", framealpha=0.9)
    fig.tight_layout()
    path = os.path.join(output_dir, "time_comparison.png")
    fig.savefig(path, dpi=150, bbox_inches="tight")
    plt.close(fig)
    print(f"  Saved {path}")


# ── Graph 3: RAM Reduction % ─────────────────────────────────────────────

def plot_memory_reduction(results, output_dir):
    fig, ax = plt.subplots(figsize=(12, 6))

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
                markersize=8, linewidth=2.5, label=label)

        # Annotate the last point
        ax.annotate(f"{reductions[-1]:.0f}%", (rows[-1], reductions[-1]),
                    textcoords="offset points", xytext=(10, -5),
                    fontsize=10, fontweight="bold", color=color)

    ax.set_xscale("log")
    ax.set_xlabel("Number of Rows")
    ax.set_ylabel("RAM Reduction (%)")
    ax.set_title("Memory Reduction: Optimized vs Original")
    ax.set_ylim(0, 100)
    ax.axhline(y=50, color="gray", linestyle=":", alpha=0.3)
    ax.axhline(y=75, color="gray", linestyle=":", alpha=0.3)

    ax.xaxis.set_major_formatter(ticker.FuncFormatter(lambda x, _: format_rows(int(x))))

    ax.legend(loc="lower right", framealpha=0.9)
    fig.tight_layout()
    path = os.path.join(output_dir, "memory_reduction.png")
    fig.savefig(path, dpi=150, bbox_inches="tight")
    plt.close(fig)
    print(f"  Saved {path}")


# ── Graph 4: Speedup Factor ──────────────────────────────────────────────

def plot_speedup(results, output_dir):
    fig, ax = plt.subplots(figsize=(12, 6))

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
                markersize=8, linewidth=2.5, label=label)

        ax.annotate(f"{speedups[-1]:.1f}x", (rows[-1], speedups[-1]),
                    textcoords="offset points", xytext=(10, -5),
                    fontsize=10, fontweight="bold", color=color)

    ax.set_xscale("log")
    ax.set_xlabel("Number of Rows")
    ax.set_ylabel("Speedup Factor (original time / optimized time)")
    ax.set_title("Compilation Speedup: Optimized vs Original")
    ax.axhline(y=1, color="gray", linestyle="-", alpha=0.3, linewidth=1)
    ax.axhline(y=2, color="gray", linestyle=":", alpha=0.3)
    ax.axhline(y=3, color="gray", linestyle=":", alpha=0.3)

    ax.xaxis.set_major_formatter(ticker.FuncFormatter(lambda x, _: format_rows(int(x))))

    ax.legend(loc="upper left", framealpha=0.9)
    fig.tight_layout()
    path = os.path.join(output_dir, "speedup.png")
    fig.savefig(path, dpi=150, bbox_inches="tight")
    plt.close(fig)
    print(f"  Saved {path}")


# ── Graph 5: Scaling — RAM per 1K rows ────────────────────────────────────

def plot_scaling(results, output_dir):
    fig, axes = plt.subplots(1, 2, figsize=(16, 6))

    templates = sorted(set(r["template"] for r in results))

    # Left: RAM per 1K rows
    ax = axes[0]
    for tname in templates:
        for binary in ["original", "optimized"]:
            pts = sorted(
                [r for r in results if r["template"] == tname and r["binary"] == binary],
                key=lambda r: r["rows"]
            )
            # Only show sizes >= 1K for per-row metrics
            pts = [p for p in pts if p["rows"] >= 1000]
            if not pts:
                continue
            rows = [p["rows"] for p in pts]
            per_k = [p["peak_ram_mb"] / (p["rows"] / 1000) for p in pts]
            style = "-" if binary == "optimized" else "--"
            marker = MARKERS.get(tname, "o")
            color = COLORS.get(tname, "#333")
            alpha = 1.0 if binary == "optimized" else 0.6
            label = f"{TEMPLATE_LABELS.get(tname, tname)} ({binary})"
            ax.plot(rows, per_k, style, marker=marker, color=color, alpha=alpha,
                    markersize=6, linewidth=1.5, label=label)

    ax.set_xscale("log")
    ax.set_xlabel("Number of Rows")
    ax.set_ylabel("MB per 1K Rows")
    ax.set_title("Memory Efficiency: RAM per 1K Rows")
    ax.xaxis.set_major_formatter(ticker.FuncFormatter(lambda x, _: format_rows(int(x))))
    ax.legend(loc="upper right", fontsize=8, framealpha=0.9)

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
            alpha = 1.0 if binary == "optimized" else 0.6
            label = f"{TEMPLATE_LABELS.get(tname, tname)} ({binary})"
            ax.plot(rows, per_k, style, marker=marker, color=color, alpha=alpha,
                    markersize=6, linewidth=1.5, label=label)

    ax.set_xscale("log")
    ax.set_xlabel("Number of Rows")
    ax.set_ylabel("Seconds per 1K Rows")
    ax.set_title("Time Efficiency: Seconds per 1K Rows")
    ax.xaxis.set_major_formatter(ticker.FuncFormatter(lambda x, _: format_rows(int(x))))
    ax.legend(loc="upper right", fontsize=8, framealpha=0.9)

    fig.tight_layout()
    path = os.path.join(output_dir, "scaling.png")
    fig.savefig(path, dpi=150, bbox_inches="tight")
    plt.close(fig)
    print(f"  Saved {path}")


# ── Graph 6: Optimized-only scaling for large sizes ───────────────────────

def plot_optimized_scaling(results, output_dir):
    """Show how the optimized binary scales to 1.2M rows."""
    opt_results = [r for r in results if r["binary"] == "optimized"]
    if not opt_results:
        return

    fig, axes = plt.subplots(1, 2, figsize=(14, 6))
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
                markersize=8, linewidth=2.5, label=label)

        # Annotate last point
        if ram:
            ax.annotate(f"{ram[-1]:,.0f} MB", (rows[-1], ram[-1]),
                        textcoords="offset points", xytext=(-60, 10),
                        fontsize=9, fontweight="bold", color=color)

    ax.set_xscale("log")
    ax.set_xlabel("Number of Rows")
    ax.set_ylabel("Peak RAM (MB)")
    ax.set_title("Optimized Binary: Memory Scaling")
    ax.xaxis.set_major_formatter(ticker.FuncFormatter(lambda x, _: format_rows(int(x))))
    ax.legend(loc="upper left", framealpha=0.9)

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
                markersize=8, linewidth=2.5, label=label)

        if times:
            t = times[-1]
            lbl = f"{t/60:.0f}m" if t >= 60 else f"{t:.0f}s"
            ax.annotate(lbl, (rows[-1], times[-1]),
                        textcoords="offset points", xytext=(-50, 10),
                        fontsize=9, fontweight="bold", color=color)

    ax.set_xscale("log")
    ax.set_xlabel("Number of Rows")
    ax.set_ylabel("Compilation Time (seconds)")
    ax.set_title("Optimized Binary: Time Scaling")
    ax.xaxis.set_major_formatter(ticker.FuncFormatter(lambda x, _: format_rows(int(x))))
    ax.legend(loc="upper left", framealpha=0.9)

    fig.tight_layout()
    path = os.path.join(output_dir, "optimized_scaling.png")
    fig.savefig(path, dpi=150, bbox_inches="tight")
    plt.close(fig)
    print(f"  Saved {path}")


# ── Graph 7: Bar chart — head-to-head at key sizes ───────────────────────

def plot_bar_comparison(results, output_dir):
    """Side-by-side bar chart at key comparison sizes."""
    templates = sorted(set(r["template"] for r in results))

    # Find sizes where we have both original and optimized
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
    # Pick at most 5 representative sizes for readability
    if len(sizes) > 5:
        indices = np.linspace(0, len(sizes)-1, 5, dtype=int)
        sizes = [sizes[i] for i in indices]

    fig, axes = plt.subplots(1, 2, figsize=(16, 7))

    # Memory bars
    ax = axes[0]
    x = np.arange(len(sizes))
    width = 0.35
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

        offset = (i - len(templates)/2 + 0.5) * width * 0.6
        bars1 = ax.bar(x + offset - width*0.15, orig_ram, width*0.3,
                       label=f"{TEMPLATE_LABELS.get(tname, tname)} (original)",
                       color=COLORS.get(tname, "#333"), alpha=0.4, edgecolor="white")
        bars2 = ax.bar(x + offset + width*0.15, opt_ram, width*0.3,
                       label=f"{TEMPLATE_LABELS.get(tname, tname)} (optimized)",
                       color=COLORS.get(tname, "#333"), alpha=0.9, edgecolor="white")

    ax.set_xlabel("Number of Rows")
    ax.set_ylabel("Peak RAM (MB)")
    ax.set_title("Memory: Original (faded) vs Optimized (solid)")
    ax.set_xticks(x)
    ax.set_xticklabels([format_rows(s) for s in sizes])
    ax.legend(fontsize=8, loc="upper left")

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

        offset = (i - len(templates)/2 + 0.5) * width * 0.6
        ax.bar(x + offset - width*0.15, orig_time, width*0.3,
               label=f"{TEMPLATE_LABELS.get(tname, tname)} (original)",
               color=COLORS.get(tname, "#333"), alpha=0.4, edgecolor="white")
        ax.bar(x + offset + width*0.15, opt_time, width*0.3,
               label=f"{TEMPLATE_LABELS.get(tname, tname)} (optimized)",
               color=COLORS.get(tname, "#333"), alpha=0.9, edgecolor="white")

    ax.set_xlabel("Number of Rows")
    ax.set_ylabel("Compilation Time (seconds)")
    ax.set_title("Time: Original (faded) vs Optimized (solid)")
    ax.set_xticks(x)
    ax.set_xticklabels([format_rows(s) for s in sizes])
    ax.legend(fontsize=8, loc="upper left")

    fig.tight_layout()
    path = os.path.join(output_dir, "bar_comparison.png")
    fig.savefig(path, dpi=150, bbox_inches="tight")
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
