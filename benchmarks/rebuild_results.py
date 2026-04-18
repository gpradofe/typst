"""Rebuild benchmark_results.json with updated optimized numbers.

Takes the original binary results from the existing file, replaces all optimized
results with fresh post-fix data from multiple source files.
"""
import json
import sys
import os

def load_results(path):
    with open(path) as f:
        data = json.load(f)
    return data

def main():
    base = os.path.dirname(os.path.abspath(__file__))
    tests_dir = os.path.join(os.path.dirname(base), "..", "tests")

    # Source 1: existing merged file (for original results + metadata)
    existing = load_results(os.path.join(base, "benchmark_results.json"))

    # Keep only original results from existing
    original_results = [r for r in existing["results"] if r["binary"] == "original"]
    print(f"Original results: {len(original_results)}")

    # Source 2: post-fix 100-100K optimized (all 3 templates)
    opt_new = load_results(os.path.join(base, "benchmark_results_opt_new.json"))
    opt_100k = [r for r in opt_new["results"]]
    print(f"Post-fix 100-100K optimized: {len(opt_100k)}")

    # Source 3: post-fix 300K-600K optimized (all 3 templates)
    opt_large_path = os.path.join(base, "benchmark_results_opt_large.json")
    if os.path.exists(opt_large_path):
        opt_large = load_results(opt_large_path)
        opt_300_600 = [r for r in opt_large["results"]]
        print(f"Post-fix 300K-600K optimized: {len(opt_300_600)}")
    else:
        print(f"WARNING: {opt_large_path} not found, using existing optimized 300K-600K")
        opt_300_600 = [r for r in existing["results"]
                       if r["binary"] == "optimized" and r["rows"] in (300000, 600000)]

    # Source 4: 1.2M optimized (unchanged by fix — single large grids)
    opt_1m = [r for r in existing["results"]
              if r["binary"] == "optimized" and r["rows"] == 1200000]
    print(f"1.2M optimized (unchanged): {len(opt_1m)}")

    # Merge all
    all_results = original_results + opt_100k + opt_300_600 + opt_1m

    # Deduplicate: keep last entry for each (binary, template, rows) combination
    seen = {}
    for r in all_results:
        key = (r["binary"], r["template"], r["rows"])
        seen[key] = r
    deduped = list(seen.values())

    # Sort: original first, then optimized; within each, by template then rows
    template_order = {"simple": 0, "single-table-advanced": 1, "multi-table": 2}
    deduped.sort(key=lambda r: (
        0 if r["binary"] == "original" else 1,
        template_order.get(r["template"], 99),
        r["rows"]
    ))

    merged = {
        "system": existing["system"],
        "templates": existing["templates"],
        "binaries": existing.get("binaries", {}),
        "results": deduped,
    }

    output = os.path.join(base, "benchmark_results.json")
    with open(output, "w") as f:
        json.dump(merged, f, indent=2)

    print(f"\nWrote {len(deduped)} results to {output}")

    # Summary
    for binary in ["original", "optimized"]:
        br = [r for r in deduped if r["binary"] == binary]
        print(f"\n{binary.upper()} ({len(br)} results):")
        for r in br:
            print(f"  {r['template']:25s} {r['rows']:>10,} rows  "
                  f"{r['peak_ram_mb']:>8.0f} MB  {r['time_s']:>8.1f}s")

if __name__ == "__main__":
    main()
