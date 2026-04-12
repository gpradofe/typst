"""
Comprehensive benchmark runner: measures peak RSS and wall-clock time for
both the original and optimized Typst binaries across multiple templates
and data sizes.

Saves raw results to benchmark_results.json for graphing.

Usage:
    python run_benchmarks.py                    # Run all benchmarks
    python run_benchmarks.py --quick            # Subset (100..100K only)
    python run_benchmarks.py --opt-only         # Only optimized binary
    python run_benchmarks.py --sizes 100 10000  # Specific sizes
"""
import subprocess
import sys
import time
import os
import json
import threading
import argparse
import platform
import datetime
import io

os.environ["PYTHONUTF8"] = "1"

# Force UTF-8 stdout on Windows
if sys.platform == "win32":
    sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding="utf-8", errors="replace")
    sys.stderr = io.TextIOWrapper(sys.stderr.buffer, encoding="utf-8", errors="replace")

try:
    import psutil
except ImportError:
    print("ERROR: psutil required. Install with: pip install psutil")
    sys.exit(1)

# ── Paths ──────────────────────────────────────────────────────────────────

BASE = os.path.dirname(os.path.abspath(__file__))
PROJECT = os.path.dirname(BASE)

ORIGINAL = os.path.join(PROJECT, "typst-bin", "typst-x86_64-pc-windows-msvc", "typst.exe")
OPTIMIZED = os.path.join(PROJECT, "typst-source", "target", "release", "typst.exe")

# ── Templates ──────────────────────────────────────────────────────────────

TEMPLATES = {
    "simple": {
        "file": os.path.join(BASE, "table_test.typ"),
        "data_format": "simple",
        "description": "Plain 10-column table, no styling",
    },
    "single-table-advanced": {
        "file": os.path.join(BASE, "single_table_advanced_test.typ"),
        "data_format": "advanced",
        "description": "Single giant table with group headers, styling, page headers/footers",
    },
    "multi-table": {
        "file": os.path.join(BASE, "advanced_table_test.typ"),
        "data_format": "advanced",
        "description": "Multiple tables (one per group), headers/footers, alternating fills",
    },
}

# ── Sizes ──────────────────────────────────────────────────────────────────

ALL_SIZES = [100, 1_000, 10_000, 50_000, 100_000, 300_000, 600_000, 1_200_000]
QUICK_SIZES = [100, 1_000, 10_000, 50_000, 100_000]

# Original binary can't handle very large documents (runs out of memory / too slow).
# We set per-size timeouts and skip the original for very large sizes.
ORIGINAL_MAX_SIZE = 300_000    # Don't run original above this
TIMEOUT_PER_SIZE = {
    100: 60,
    1_000: 120,
    10_000: 300,
    50_000: 600,
    100_000: 1200,
    300_000: 2400,
    600_000: 3600,
    1_200_000: 7200,
}

# ── Memory monitoring ─────────────────────────────────────────────────────

def monitor_memory(proc, result, interval=0.02):
    """Monitor peak RSS of a process and its children."""
    peak_rss = 0
    samples = 0
    try:
        p = psutil.Process(proc.pid)
        while proc.poll() is None:
            try:
                mem = p.memory_info().rss
                for child in p.children(recursive=True):
                    try:
                        mem += child.memory_info().rss
                    except (psutil.NoSuchProcess, psutil.AccessDenied):
                        pass
                if mem > peak_rss:
                    peak_rss = mem
                samples += 1
            except (psutil.NoSuchProcess, psutil.AccessDenied):
                break
            time.sleep(interval)
    except Exception:
        pass
    result["peak_rss"] = peak_rss
    result["samples"] = samples


def run_test(typst_exe, typ_file, datafile, output_pdf, timeout=600, cwd=None):
    """Run a single typst compile and return metrics."""
    cmd = [typst_exe, "compile", typ_file, output_pdf,
           "--input", f"datafile={datafile}"]
    result = {"peak_rss": 0, "samples": 0}
    start = time.time()
    try:
        proc = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, cwd=cwd)
        monitor = threading.Thread(target=monitor_memory, args=(proc, result, 0.02))
        monitor.start()
        stdout, stderr = proc.communicate(timeout=timeout)
        elapsed = time.time() - start
        monitor.join(timeout=5)

        full_pdf = os.path.join(cwd, output_pdf) if cwd else output_pdf
        pdf_size = os.path.getsize(full_pdf) if os.path.exists(full_pdf) else 0

        return {
            "ok": proc.returncode == 0,
            "time_s": round(elapsed, 2),
            "peak_ram_mb": round(result["peak_rss"] / (1024 * 1024), 1),
            "peak_ram_bytes": result["peak_rss"],
            "pdf_size_mb": round(pdf_size / (1024 * 1024), 2),
            "pdf_size_bytes": pdf_size,
            "samples": result["samples"],
            "error": stderr.decode(errors="replace")[:500] if proc.returncode != 0 else None,
        }
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.communicate()
        elapsed = time.time() - start
        return {
            "ok": False,
            "time_s": round(elapsed, 2),
            "peak_ram_mb": round(result["peak_rss"] / (1024 * 1024), 1),
            "peak_ram_bytes": result["peak_rss"],
            "pdf_size_mb": 0,
            "pdf_size_bytes": 0,
            "samples": result["samples"],
            "error": f"TIMEOUT after {timeout}s",
        }
    finally:
        full_pdf = os.path.join(cwd, output_pdf) if cwd else output_pdf
        if os.path.exists(full_pdf):
            try:
                os.remove(full_pdf)
            except OSError:
                pass


# ── Main ───────────────────────────────────────────────────────────────────

def main():
    parser = argparse.ArgumentParser(description="Run Typst benchmarks")
    parser.add_argument("--quick", action="store_true", help="Quick mode (up to 100K only)")
    parser.add_argument("--opt-only", action="store_true", help="Only run optimized binary")
    parser.add_argument("--orig-only", action="store_true", help="Only run original binary")
    parser.add_argument("--sizes", nargs="+", type=int, help="Specific sizes to test")
    parser.add_argument("--templates", nargs="+", help="Specific templates to test")
    parser.add_argument("--output", default=os.path.join(BASE, "benchmark_results.json"),
                        help="Output JSON file")
    parser.add_argument("--runs", type=int, default=1, help="Number of runs per config (for averaging)")
    args = parser.parse_args()

    sizes = args.sizes or (QUICK_SIZES if args.quick else ALL_SIZES)
    template_names = args.templates or list(TEMPLATES.keys())

    # Validate
    for t in template_names:
        if t not in TEMPLATES:
            print(f"Unknown template: {t}")
            sys.exit(1)

    binaries = []
    if not args.opt_only:
        if os.path.exists(ORIGINAL):
            binaries.append(("original", ORIGINAL))
        else:
            print(f"WARNING: Original binary not found at {ORIGINAL}")
    if not args.orig_only:
        if os.path.exists(OPTIMIZED):
            binaries.append(("optimized", OPTIMIZED))
        else:
            print(f"WARNING: Optimized binary not found at {OPTIMIZED}")

    if not binaries:
        print("ERROR: No binaries found to test")
        sys.exit(1)

    # Collect system info
    system_info = {
        "platform": platform.platform(),
        "processor": platform.processor(),
        "cpu_count": os.cpu_count(),
        "total_ram_gb": round(psutil.virtual_memory().total / (1024**3), 1),
        "python_version": platform.python_version(),
        "timestamp": datetime.datetime.now().isoformat(),
    }

    print("=" * 80)
    print("TYPST BENCHMARK SUITE")
    print("=" * 80)
    print(f"System: {system_info['platform']}")
    print(f"CPU: {system_info['processor']} ({system_info['cpu_count']} cores)")
    print(f"RAM: {system_info['total_ram_gb']} GB")
    print(f"Sizes: {', '.join(f'{s:,}' for s in sizes)}")
    print(f"Templates: {', '.join(template_names)}")
    print(f"Binaries: {', '.join(b[0] for b in binaries)}")
    print(f"Runs per config: {args.runs}")
    print("=" * 80)

    results = []
    total_tests = len(sizes) * len(template_names) * len(binaries)
    test_num = 0

    for size in sizes:
        label = f"{size // 1000}k" if size >= 1000 else str(size)

        for tname in template_names:
            tmpl = TEMPLATES[tname]
            fmt = tmpl["data_format"]

            # Determine data file path
            if fmt == "simple":
                # Map old naming: data_tiny=100, data_small=1000, data_medium=10000, data_large=100000
                old_names = {100: "tiny", 1000: "small", 10000: "medium", 100000: "large",
                             500000: "xlarge", 1000000: "massive"}
                if size in old_names:
                    datafile = os.path.join(BASE, f"data_{old_names[size]}.json")
                    if not os.path.exists(datafile):
                        datafile = os.path.join(BASE, f"data_{label}.json")
                else:
                    datafile = os.path.join(BASE, f"data_{label}.json")
            else:
                old_names = {100: "tiny", 1000: "small", 10000: "medium"}
                if size in old_names:
                    datafile = os.path.join(BASE, f"data_advanced_{old_names[size]}.json")
                    if not os.path.exists(datafile):
                        datafile = os.path.join(BASE, f"data_advanced_{label}.json")
                else:
                    datafile = os.path.join(BASE, f"data_advanced_{label}.json")

            if not os.path.exists(datafile):
                print(f"\n  SKIP {tname} @ {size:,} — data file not found: {os.path.basename(datafile)}")
                continue

            data_size_mb = round(os.path.getsize(datafile) / (1024 * 1024), 2)

            if not os.path.exists(tmpl["file"]):
                print(f"\n  SKIP {tname} — template not found")
                continue

            for bname, bpath in binaries:
                # Skip original for very large sizes
                if bname == "original" and size > ORIGINAL_MAX_SIZE:
                    test_num += 1
                    print(f"\n[{test_num}/{total_tests}] SKIP {bname} / {tname} @ {size:,} rows (too large for original)")
                    results.append({
                        "binary": bname,
                        "template": tname,
                        "rows": size,
                        "data_size_mb": data_size_mb,
                        "skipped": True,
                        "reason": f"Size {size:,} exceeds original binary limit ({ORIGINAL_MAX_SIZE:,})",
                    })
                    continue

                timeout = TIMEOUT_PER_SIZE.get(size, 3600)

                for run_i in range(args.runs):
                    test_num += 1
                    run_label = f" (run {run_i+1}/{args.runs})" if args.runs > 1 else ""
                    print(f"\n[{test_num}/{total_tests}] {bname} / {tname} @ {size:,} rows{run_label}")
                    print(f"  Data: {os.path.basename(datafile)} ({data_size_mb} MB)")
                    print(f"  Timeout: {timeout}s")

                    out_pdf = f"_bench_{bname}_{tname}_{label}.pdf"
                    # Use relative paths and run from the tests dir to avoid
                    # Windows backslash issues in Typst's --input path.
                    rel_typ = os.path.basename(tmpl["file"])
                    rel_data = os.path.basename(datafile)
                    stats = run_test(bpath, rel_typ, rel_data, out_pdf,
                                     timeout=timeout, cwd=BASE)

                    if stats["ok"]:
                        ram_ratio = round(stats["peak_ram_mb"] / data_size_mb, 1) if data_size_mb > 0.01 else 0
                        print(f"  Time:     {stats['time_s']:.1f}s")
                        print(f"  Peak RAM: {stats['peak_ram_mb']:.0f} MB")
                        print(f"  PDF size: {stats['pdf_size_mb']:.1f} MB")
                        print(f"  RAM/data: {ram_ratio}x")
                    else:
                        print(f"  FAILED: {stats.get('error', 'unknown')[:200]}")

                    results.append({
                        "binary": bname,
                        "template": tname,
                        "rows": size,
                        "data_size_mb": data_size_mb,
                        "run": run_i,
                        "skipped": False,
                        **stats,
                    })

    # Save results
    output = {
        "system": system_info,
        "templates": {k: {"description": v["description"], "data_format": v["data_format"]}
                      for k, v in TEMPLATES.items() if k in template_names},
        "binaries": {b[0]: b[1] for b in binaries},
        "results": results,
    }

    with open(args.output, "w") as f:
        json.dump(output, f, indent=2)

    print(f"\n\nResults saved to {args.output}")

    # Print summary table
    print("\n" + "=" * 100)
    print("SUMMARY")
    print("=" * 100)
    print(f"{'Template':<25} {'Rows':>10} {'Orig RAM':>10} {'Opt RAM':>10} {'Savings':>8} {'Orig Time':>10} {'Opt Time':>10} {'Speedup':>8}")
    print("-" * 100)

    for tname in template_names:
        for size in sizes:
            orig = next((r for r in results if r["binary"] == "original" and r["template"] == tname
                        and r["rows"] == size and not r.get("skipped") and r.get("ok")), None)
            opt = next((r for r in results if r["binary"] == "optimized" and r["template"] == tname
                       and r["rows"] == size and not r.get("skipped") and r.get("ok")), None)

            orig_ram = f"{orig['peak_ram_mb']:.0f} MB" if orig else "—"
            opt_ram = f"{opt['peak_ram_mb']:.0f} MB" if opt else "—"
            orig_time = f"{orig['time_s']:.1f}s" if orig else "—"
            opt_time = f"{opt['time_s']:.1f}s" if opt else "—"

            if orig and opt:
                savings = f"{(1 - opt['peak_ram_mb']/orig['peak_ram_mb'])*100:.0f}%"
                speedup = f"{orig['time_s']/opt['time_s']:.1f}x"
            else:
                savings = "—"
                speedup = "—"

            print(f"{tname:<25} {size:>10,} {orig_ram:>10} {opt_ram:>10} {savings:>8} {orig_time:>10} {opt_time:>10} {speedup:>8}")


if __name__ == "__main__":
    main()
