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
  - `md5sum`/`sha1sum`/`sha256sum`/`sha512sum`/`sha3sum` ✅ — `modern/hashsum.rs`,
    feature `modern-hashsum`. The planned dependency was uutils' `uu_hashsum`,
    but it's been stuck at 0.5 for 7+ months (checked crates.io directly)
    while the rest of the uutils family here is at 0.9 — genuinely blocked,
    not worth waiting on further. Used RustCrypto's `md-5`/`sha1`/`sha2`/
    `sha3` crates directly instead — audited, far more widely used than a
    single coreutils wrapper, and all expose the same `Digest` trait, so one
    small dispatcher covers all five algorithms. Covers plain hashing,
    `-c` check mode (both `HASH  file` and `HASH *file` separators), `-s`/
    `-w`, `-b`/`-t` (GNU no-ops), and sha3sum's `-a WIDTH` restricted to the
    four values that are actually standardized SHA3 (224/256/384/512 per
    FIPS 202) rather than the transpiled version's looser "any multiple of
    32". Verified against upstream's own `testsuite/md5sum.tests` chained-
    hash scenario (0..999-byte inputs hashed individually, then the
    concatenated results hashed again) for all five algorithms, plus direct
    cross-checks against Python's `hashlib` — all exact matches.
- **Phase 3 — no-crate applets.** Hand-written safe rewrites for the things with
  no good library (`mount`, `ifconfig`, `ip`, `init`, the `ash` shell), using
  `rustix`/`nix` for syscalls instead of the transpiled `unsafe` FFI.
  - `ifconfig` ✅ — `modern/ifconfig.rs`, feature `modern-ifconfig`. Display via
    `nix::ifaddrs::getifaddrs` (fully safe read path); set operations (address,
    netmask, broadcast, pointopoint, dstaddr, hw ether, mtu, metric,
    txqueuelen, up/down/arp/promisc/allmulti/multicast/dynamic/trailers) via a
    handful of confined `ioctl(2)` helpers instead of pointer arithmetic
    threaded through the whole applet. Out of scope: IPv6 add/del, `hw
    infiniband`, and legacy SLIP/ISA options (`mem_start`, `io_addr`, `irq`,
    `keepalive`, `outfill`) — dead hardware classes BusyBox itself already
    called out as unmaintained ("Still missing: media, tunnel").
  - `mount`/`umount`/`mountpoint` ✅ — `modern/mount.rs`, `modern/umount.rs`,
    `modern/mountpoint.rs`, features `modern-mount`/`modern-umount`/
    `modern-mountpoint`. `mount`/`umount` via `nix::mount::{mount,umount2}`
    (safe wrappers around mount(2)/umount2(2)); `mountpoint` is fully
    `unsafe`-free (`std::fs` stat calls + the glibc major/minor bit-decode
    formula, no ioctls at all). Covers two-arg mount with `-o` flags and
    fstype autodetection via `/proc/filesystems`, bind/rbind/move/remount/
    make-{shared,private,slave,unbindable} (+ recursive), `-a` against
    `/etc/fstab` or `-T FILE` with `-t`/`-O` filtering, and the bare listing.
    Out of scope: automatic loop-device attach for `mount image.img dir`
    (losetup(8) it yourself first), CIFS/NFS fstab shorthand auto-detection,
    and the `mount.<fstype>` helper-program fallback (already dead code
    upstream).
  - `ip` ✅ (partial) — `modern/ip.rs`, feature `modern-ip`. Unlike the other
    Phase 3 applets, the transpiled `ip` already has a correct, full
    netlink-based implementation (`networking/libiproute/`) — there's no bug
    to fix in the parts left uncovered, only unsafe-vs-safe style. So this
    covers just the read-only, most-used surface with zero ioctls/netlink at
    all — `ip addr show`, `ip link show` (via `getifaddrs`, same as
    `ifconfig`), `ip route show` (IPv4 only, via `/proc/net/route`) — plus
    one narrow, low-risk mutation, `ip link set IFACE up|down` (same confined
    ioctl helper pattern as `ifconfig`). `ip::run` returns `Option<i32>`
    instead of always-`Some`, so any subcommand it doesn't recognize (`addr
    add/del/change`, `link set mtu/address/…`, `route add/del`, `rule`/
    `tunnel`/`neigh`, anything with a selector this file doesn't parse)
    returns `None` and falls through to the transpiled `ip_main` per the
    same `try_run` contract `modern.rs` already uses per-applet — just
    exercised per-subcommand here. Verified e2e in the build container
    (addr/link/route show, link set up/down against `lo`, and the
    fallthrough path via `ip rule show`).
  - `init` ✅ (+ `linuxrc` alias) — `modern/init.rs`, feature `modern-init`.
    Full behavioral port of the PID-1 supervisor: inittab parsing (`id:
    runlevels:action:process`, `runlevels` parsed-but-ignored matching
    upstream, actions `sysinit`/`wait`/`once`/`respawn`/`askfirst`/
    `ctrlaltdel`/`shutdown`/`restart`), the same hardcoded default inittab
    when `/etc/inittab` is missing, spawn/respawn/zombie-reap via
    `nix::unistd::fork`/`nix::sys::wait::waitpid`, delayed-signal handling
    (SIGHUP/SIGINT/SIGQUIT/SIGTERM/SIGUSR1/SIGUSR2 recorded via atomic
    flags in an async-signal-safe handler, acted on in the main loop — same
    design as upstream's `record_signo`), and shutdown/reboot/halt/poweroff
    via the confined `reboot(2)` FFI call. Two intentional simplifications
    (documented in the module): always `fork()` instead of upstream's
    `vfork()`/`fork()` split (a NOMMU-embedded-only optimization, irrelevant
    on any MMU target this project ships for), and no SIGTSTP/SIGSTOP
    job-control freeze handler (legacy interactive-console feature). One
    intentional **hardening beyond upstream**: the PID-1 check is enforced
    unconditionally, including for `linuxrc` — upstream waives it for that
    name, which would let an accidental `rustybox linuxrc` invocation on a
    dev machine hijack the process into an infinite supervisor loop with
    hijacked signal handlers instead of safely erroring.
    Verified as **real PID 1** via `unshare --pid --mount --fork
    --mount-proc` in the build container (not testable via a plain
    `cargo test` subprocess): sysinit→once ordering, respawn-without-
    zombie-leaks over multiple cycles, the default-inittab fallback
    degrading gracefully when `rcS`/ttys are missing, and — the hardest
    path to test — a child process sending real `SIGTERM` to PID 1
    correctly triggering the full shutdown-and-reboot sequence and tearing
    down the namespace. This process caught two real bugs before they
    shipped: `linuxrc` bypassing the PID-1 check entirely (matching
    upstream's literal behavior, but dangerous — see the hardening above)
    hung a live invocation into an infinite supervisor loop; and testing
    `init -q` (which sends a real `SIGHUP` to whatever is PID 1) inside the
    test suite killed the *test container's own* PID 1 and took the whole
    suite down with it — removed from the automated suite (signal delivery
    to an unrelated PID 1 is inherently environment-dependent, matching
    a disposable namespace instead.
  - `setsid` ✅ — `modern/setsid.rs`, feature `modern-setsid`. Full memory-safe rewrite using `nix::unistd::setsid` and `fork`/`execvp`. Covers `-c` (controlling terminal) via a safe `ioctl` helper.
  - `flock` ✅ — `modern/flock.rs`, feature `modern-flock`. Full memory-safe rewrite using `nix::libc::flock`. Covers shared, exclusive, non-blocking, and unlock semantics, as well as executing a command with or without `-c`.
  - `chrt` ✅ — `modern/chrt.rs`, feature `modern-chrt`. Full memory-safe rewrite using `nix::libc::sched_*`. Covers scheduling policy getting/setting and execution.
  - `ash` — full grammar/parser/executor rewrite ruled out for now (uses
    `setjmp`/`longjmp` throughout for error propagation, which doesn't map
    onto Rust's Drop model; `testsuite/ash.tests` has zero real shell-
    semantics coverage — by its own comment it only tests line-editing/
    unicode display). Doing incremental hardening in place instead: keep
    the transpiled control flow, replace individual unsafe syscalls with
    `nix` where it's a genuine improvement. First increment ✅ (see
    `docs/superpowers/specs/2026-07-21-ash-job-control-hardening-design.md`):
    `fork`/`setpgid`/`tcsetpgrp`/`tcgetpgrp`/`getpgrp`/`getppid` (10 call
    sites) now go through `nix::unistd`. Deferred to a separate pass:
    `sigaction`/`signal`/`sigfillset`/`sigsuspend` — entangled with
    `sigprocmask`/`sigprocmask2` (not originally scoped) and an inherited-
    `SIG_IGN` detection query nix's `sigaction()` doesn't cleanly support
    without its own design pass. `pipe`/`waitpid`/`killpg`/`raise`/`execve`
    also left as direct libc calls (real risk of shuffling unsafety around
    rather than reducing it — see the design spec for the per-function
    reasoning). The lexer/parser/expansion/executor/~40 builtins are
    entirely untouched.
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

## Harness-useful tools (agentic / Claude Code)

Modern agent harnesses lean on a specific slice of the userland. Where the
applet already exists in BusyBox, we route it to a memory-safe backend; the
standouts are wired now:

| applet | why harnesses want it | backend |
|---|---|---|
| **timeout** | bound runaway commands — the single most useful guard for autonomous loops (`timeout 30 <cmd>`) | uu_timeout ✅ |
| **nohup** | detach long-running work from the session | uu_nohup ✅ |
| **dd** | byte-exact I/O, image/stream work | uu_dd ✅ |
| **shuf, nice, truncate, fold, expand, unexpand, comm, split, cksum, paste, sync, uname** | data munging + housekeeping | uutils ✅ |
| env, seq, sleep, tee, mktemp, realpath, nproc, printf | scripting glue | uutils ✅ (Phase 1) |

**Stay transpiled for now** (util-linux/procps, no coreutils crate): `xargs`
(fan-out), `watch`, `ionice`. Candidates for
in-house safe rewrites (Phase 3) or a dedicated crate.

**Net-new applets** — not in BusyBox, so they need an `applet_tables.rs` entry
in addition to a backend. High-value future adds:

- `stdbuf` (uu_stdbuf) — unbuffer child output; very useful for streaming logs
  from agent-spawned processes.
- `numfmt` (uu_numfmt), `tsort` (uu_tsort), `join` (uu_join) — coreutils that
  BusyBox omits.
- Optional "modern CLI" layer (own applet names, not BusyBox): `rg` (ripgrep),
  `fd`, `bat`, `jq`→`jaq` (pure-Rust jq), `hyperfine`, `zoxide`. These are
  developer-facing niceties; gate behind a separate `extras` feature so the
  BusyBox-compatible core stays clean.

## Success criteria

A migrated applet is "done" when: its `*-modern` backend is the default feature,
the transpiled `*_main` and its `unsafe` are deleted, and the `testsuite/` cases
for it pass identically to BusyBox on both `x86_64` and `aarch64` musl.
