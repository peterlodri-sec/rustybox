# rustybox

[![Sponsor](https://img.shields.io/badge/Sponsor-%E2%9D%A4-db61a2?logo=githubsponsors&logoColor=white)](https://github.com/sponsors/peterlodri-sec)
[![License](https://img.shields.io/badge/license-GPL--2.0%20%2F%20MIT%20core-blue)](LICENSE)
[![Static](https://img.shields.io/badge/build-static%20musl%20·%20x86__64%20%2B%20aarch64-2ee6a6)](#static-dual-architecture-binaries)
[![site](https://img.shields.io/badge/site-rustybox.io-6cf)](https://rustybox.io)

> **Built in the open — [♥ sponsor the resurrection](https://github.com/sponsors/peterlodri-sec).**

RustyBox is a free-range, non-GMO fork of [BusyBox](https://busybox.net/) written entirely in [Rust](https://www.rust-lang.org/). It includes all your favorite commands like `ls`, `mount`, and `top`, but without a single line of C code. Like BusyBox, it fits in about a megabyte and covers the basic utilities you need to stand up a small Linux userland.

## Status

**Resurrected (2026-07).** rustybox started as a direct [c2rust](https://github.com/immunant/c2rust) transpile of BusyBox and then sat untouched from 2020, when it stopped compiling on any modern Rust (`llvm_asm!`, removed nightly features, an ancient `libc`). It now builds, links, and runs again:

- Compiles with a **current nightly** (pinned in `rust-toolchain.toml`), edition 2021.
- **All applets compile** (`cargo build --all-features`).
- **Fully-static musl binaries** for both **x86_64** and **aarch64** — the same source is portable across `glibc`/`musl` and both architectures.
- `libc` bumped from the 2019-era `0.2.65` to current.

It is still "bug-for-bug compatible" with BusyBox in the sense that the internals are the transpiled C: raw pointers and `unsafe` abound. Making it idiomatic and memory-safe — applet by applet — is the ongoing work (see [MIGRATION.md](MIGRATION.md)).

## Building

rustybox is Linux-only (raw syscalls, Linux headers). On macOS/Windows, build in the provided container — it works identically everywhere.

```sh
# one-time: build the Linux build image
docker build -t rustybox-build:latest -f docker/Dockerfile .

# build everything / a curated core / run the tests
docker/build.sh --all-features
docker/build.sh                     # curated-core feature set
docker/build.sh test
```

Features only gate the applet **dispatch table** (`applets/applet_tables.rs`), not module compilation — the whole tree always compiles. Pick the applets you want in the final binary:

```sh
cargo build --release --features "cat ls which"
```

### Static, dual-architecture binaries

The musl targets produce the fully-static, embeddable binaries. They link with `rust-lld`, so you can cross-build any arch from any host (no per-arch cross-`cc`):

```sh
cargo build --release --target x86_64-unknown-linux-musl  --all-features
cargo build --release --target aarch64-unknown-linux-musl --all-features
```

`strip` the result if you are size-conscious; a curated core lands well under a megabyte.

## Roadmap

- **Idiomatic core** — replace the transpiled `unsafe` internals of the common applets with safe Rust (including `init`, `ifconfig`, and `ash` job-control), and trim the inherited warning pile.
- **Modern equivalents** — where a best-in-class Rust CLI already exists (`ripgrep`, `bat`, `fd`, `eza`, `uutils`…), offer it as a drop-in behind the familiar applet name. See [MIGRATION.md](MIGRATION.md).

## Sponsor

rustybox is built in the open — resurrecting dead code, hardening it across architectures, and making the classic Unix toolbox memory-safe. If it saves you a dependency, a container megabyte, or a `timeout` guard around a runaway command, consider backing the work.

### [→ Sponsor on GitHub](https://github.com/sponsors/peterlodri-sec)

One [GitHub Sponsors](https://github.com/sponsors/peterlodri-sec) listing funds
this alongside my other open-source work (crabcc, Vaked, …) — the tiers below
are shared across all of it, with rustybox-specific perks layered on top.

| tier | / month | you get |
|---|---|---|
| 🌱 **Supporter** | $5 | Name in [`SPONSORS.md`](SPONSORS.md) + the warm glow of funding safe systems software |
| 🔧 **Contributor** | $25 | Above, plus a monthly research-experiment run on the wider stack |
| 🏗️ **Backer** | $100 | Above, plus your name/handle in *this* README |
| 🚀 **Sponsor** | $500 | Above, plus your logo on [rustybox.io](https://rustybox.io) |
| 🤝 **Partner** | $2,500 | Above, plus prominent logo placement and a say in which applets go memory-safe next |

One-time contributions welcome too. Every sponsor is credited automatically
from their GitHub Sponsors tier (opt out anytime by replying to the welcome
message — see [SPONSORS.md](SPONSORS.md)).

<!-- sponsors:backers:start -->
_Be the first._
<!-- sponsors:backers:end -->

## Editions & licensing

rustybox ships in two editions:

- **`rustybox` (full)** — the complete BusyBox-lineage toolbox (300+ applets, incl. `awk`, `ash`, `vi`, networking, archives). Because it descends from BusyBox (transpiled via c2rust), it is a **GPL-2.0-only** derivative and stays GPLv2. This is the default binary.
- **`rustybox-core` (MIT)** — a memory-safe multicall built **entirely on permissive crates** (the [uutils](https://github.com/uutils/coreutils) coreutils, [ripgrep](https://github.com/BurntSushi/ripgrep)'s search libraries, `walkdir`/`globset`) plus rustybox's own dispatch/`grep`/`find` code. **No BusyBox/GPL code is compiled**, so this binary is distributable under **MIT**. Covers the ~66 migrated applets (coreutils family + `grep`/`find` + agent tools like `timeout`).

```sh
cargo build -p rustybox-core --release        # the MIT edition
target/release/rustybox-core grep -rn TODO .   # multicall; also works via symlinks
```

Which to use: want maximum parity → `rustybox` (GPLv2). Want an MIT-licensable, dependency-clean toolbox for embedding/redistribution → `rustybox-core`.

## Acknowledgements

None of this exists without the [BusyBox](https://busybox.net/) and [c2rust](https://github.com/immunant/c2rust) teams. Much of the code here is transpiled from the work of the BusyBox [AUTHORS](https://github.com/mirror/busybox/blob/master/AUTHORS).
