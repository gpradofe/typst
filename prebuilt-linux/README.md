# Prebuilt Linux x86_64 binary

`typst` here is an **optimized release build** of this branch
(`feat/memory-optimization-stack`, commit `e5fbc16ff` — Part 1 deferred-wrapper
+ the parallel pre-compute header-skip), built with `cargo build --release`
(opt-level 3) for x86_64 Linux (glibc).

## Use it

```bash
chmod +x typst
./typst --version

# run your report (capture wall/CPU%/RSS):
/usr/bin/time -v ./typst compile --root . --no-pdf-tags template-spf.typ out.pdf \
  | grep -E 'Percent of CPU|Elapsed|Maximum resident'

# optional: match a jemalloc setup
LD_PRELOAD=/usr/lib/x86_64-linux-gnu/libjemalloc.so.2 ./typst compile --root . --no-pdf-tags template-spf.typ out.pdf
```

## Why this exists

To take the **local build mode** out of the equation. A locally-built **debug**
binary (`cargo build` without `--release`) is ~10× slower and uses more RAM than
this one. If this prebuilt binary is fast on the real data but a local build is
slow, the local build was unoptimized — rebuild with `--release`.

`--version` cosmetically prints `74e3cc7d`; the binary does contain the
header-skip change (typst-layout was recompiled at `e5fbc16ff`).

Also available as a GitHub Release asset:
<https://github.com/gpradofe/typst/releases/tag/linux-x86_64-e5fbc16f>
