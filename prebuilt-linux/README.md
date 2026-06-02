# Prebuilt Linux x86_64 binary (RHEL 8 compatible)

`typst` here is an **optimized release build** of this branch
(`feat/memory-optimization-stack` — Part 1 deferred-wrapper + the parallel
pre-compute header-skip), built with `cargo build --release --features vendor-openssl`.

Built on **AlmaLinux 8 (glibc 2.28)** with **statically-vendored OpenSSL**, so it has:
- max required symbol `GLIBC_2.28` → runs on **RHEL/CentOS/Rocky/Alma 8** (and any glibc ≥ 2.28)
- **no `libssl.so` dependency** (OpenSSL is linked statically)

`ldd` shows only `libc/libm/libpthread/libdl/libgcc_s` — nothing else.

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

`--version` prints `(unknown commit)` only because the build host had no git
checkout metadata; the binary is built from this branch's latest commit and
contains all of its changes.

Also available as a GitHub Release asset:
<https://github.com/gpradofe/typst/releases/tag/linux-x86_64-e5fbc16f>
