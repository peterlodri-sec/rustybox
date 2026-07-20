# Migrating applets to modern Rust

rustybox today is a c2rust transpile of BusyBox: correct-ish, but pervasively
`unsafe`, raw-pointer C-in-Rust. The Rust ecosystem now has best-in-class,
memory-safe reimplementations of most of these tools. This document is the plan
for swapping the transpiled internals for those, applet by applet, **without
changing the CLI surface** — `rustybox grep` still behaves like `grep`.

## Principles

1. **One static binary.** No subprocessing out to `rg`/`fd`/`bat`. Integration
   is at the **library** level so everything links into the single multicall
   binary. This rules out tools that only ship as a `bin` (`fd`, `eza`, `bat`
   the app) unless we use their underlying library crates.
2. **CLI-compatible.** The applet name and its flags stay BusyBox-compatible.
   The modern engine is an implementation detail. Where a modern tool's default
   behavior differs (e.g. `bat` paginates/colorizes), the applet defaults to the
   plain POSIX behavior and gates extras behind flags.
3. **Feature-gated backends.** Each migrated applet gets a `<applet>-modern`
   feature. The transpiled implementation stays until the modern one reaches
   parity, so we can flip per-applet and A/B test.
4. **License.** rustybox is GPLv2 (BusyBox heritage). The target crates are
   MIT/Apache-2.0, which are GPL-compatible; the combined binary remains GPLv2.
   `uutils` is MIT. Record each crate's license in `Cargo.toml` comments.

## Dispatch architecture

BusyBox/rustybox is a multicall binary: it dispatches on `argv[0]` (or
`argv[1]`) through `applet_tables.rs` to a `<applet>_main`. Migration adds a thin
router:

```
applet name ──▶ if cfg!(feature = "<applet>-modern") ──▶ modern_backend(args) -> exit_code
                else                                  ──▶ <applet>_main(argc, argv)  // transpiled
```

The modern backend is a small adapter: `fn(args: &[OsString]) -> i32`. uutils
already exposes exactly this shape (`uu_<tool>::uumain`), so the coreutils family
is nearly free.

## Mapping

| applet(s) | modern tool | library used (not the bin) | approach |
|---|---|---|---|
| cat, ls, cp, mv, rm, mkdir, rmdir, ln, pwd, touch, wc, sort, uniq, head, tail, cut, tr, echo, printf, env, date, basename, dirname, readlink, stat, du, df, chmod, chown, seq, od, … | **uutils/coreutils** | `uu_<tool>` crates (multicall lib) | direct `uumain` adapter — **biggest win, one family** |
| grep, egrep, fgrep | **ripgrep** | `grep`, `grep-regex`, `grep-searcher`, `ignore` | build a grep applet on the grep-* libs |
| find | **fd** | `ignore`, `walkdir`, `globset` | reimplement `find` semantics on the walker libs |
| sed | **sd** (different syntax) | keep POSIX `sed`; offer `sd` as its own applet | do **not** silently replace — sed's contract differs |
| ls (rich) | **eza** | no stable lib | keep uutils `ls`; eza stays a bin, out of scope |
| cat (rich) | **bat** | `bat` crate (is a lib) | optional `cat --pretty` behind a feature; plain `cat` stays POSIX |
| od, hexdump, xxd | **hexyl** | `hexyl` (lib) | adapter for the dump applets |
| tar, gzip, bzip2, xz, cpio | rust archive crates | `tar`, `flate2`, `bzip2`, `xz2`, `zstd` | replace transpiled (de)compressors |
| ps, top | **procs / bottom** | `sysinfo`, procfs parsing | in-house, backed by `sysinfo` |
| mount, ifconfig, ip, init, ash, mdev, … | — (no drop-in) | — | **idiomatic in-house safe rewrite**; syscalls via `nix`/`rustix` |

## Phases

- **Phase 0 — router + feature scaffold.** Add the `fn(&[OsString]) -> i32`
  adapter seam and per-applet `*-modern` features. No behavior change yet.
- **Phase 1 — coreutils family (uutils).** Highest leverage: ~40 applets behind
  one mature dependency. Wire `uu_*::uumain`, diff behavior against BusyBox in
  the test suite, flip features as each passes.
- **Phase 2 — specialty engines.** grep (grep-* libs), find (`ignore`/`walkdir`),
  dump family (`hexyl`), archives (`tar`/`flate2`/…). Each is its own PR + parity
  tests.
- **Phase 3 — no-crate applets.** Hand-written safe rewrites for the things with
  no good library (`mount`, `ifconfig`, `ip`, `init`, the `ash` shell), using
  `rustix`/`nix` for syscalls instead of the transpiled `unsafe` FFI.
- **Phase 4 — retire transpiled code.** Once an applet's modern backend is the
  default and parity-tested, delete the transpiled `*_main` and its `unsafe`.

## Risks / open questions

- **Binary size.** uutils + regex + ignore pull in real dependencies. Measure
  `--features` size deltas; keep the curated-core build lean. Consider
  `opt-level="z"`, LTO (already on), and per-applet feature granularity.
- **CLI divergence.** uutils targets GNU coreutils, BusyBox is closer to POSIX
  with its own flag quirks. The test suite (`testsuite/`) is the arbiter; treat
  any diff as a bug to reconcile, not silently accept.
- **Library API stability.** ripgrep's `grep-*` crates are published and stable;
  `fd`/`eza`/`bat`-as-apps are not libraries — use their building blocks, not the
  apps.
- **Static musl.** All target crates must build for `*-linux-musl` static. Verify
  in CI on both arches (the build already does dual-arch musl).

## Success criteria

A migrated applet is "done" when: its `*-modern` backend is the default feature,
the transpiled `*_main` and its `unsafe` are deleted, and the `testsuite/` cases
for it pass identically to BusyBox on both `x86_64` and `aarch64` musl.
