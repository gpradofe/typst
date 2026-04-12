"""Merge two benchmark result files into one."""
import json
import sys
import os

def merge(file1, file2, output):
    with open(file1) as f:
        d1 = json.load(f)
    with open(file2) as f:
        d2 = json.load(f)

    merged = {
        "system": d1.get("system") or d2.get("system"),
        "templates": {**d1.get("templates", {}), **d2.get("templates", {})},
        "binaries": {**d1.get("binaries", {}), **d2.get("binaries", {})},
        "results": d1.get("results", []) + d2.get("results", []),
    }

    with open(output, "w") as f:
        json.dump(merged, f, indent=2)

    total = len(merged["results"])
    ok = sum(1 for r in merged["results"] if r.get("ok"))
    print(f"Merged {len(d1.get('results', []))} + {len(d2.get('results', []))} = {total} results ({ok} successful)")

if __name__ == "__main__":
    base = os.path.dirname(os.path.abspath(__file__))
    f1 = sys.argv[1] if len(sys.argv) > 1 else os.path.join(base, "benchmark_results_original.json")
    f2 = sys.argv[2] if len(sys.argv) > 2 else os.path.join(base, "benchmark_results.json")
    out = sys.argv[3] if len(sys.argv) > 3 else os.path.join(base, "benchmark_results_merged.json")
    merge(f1, f2, out)
