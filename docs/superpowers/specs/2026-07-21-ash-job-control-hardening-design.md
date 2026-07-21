# ash job-control/signal hardening — design

Status: approved. **Amended after the detailed implementation-planning pass**
(see the "Tier 1 / Tier 2 split" note below) — the original scope table
below undercounted the real per-call-site complexity of the signal-handling
functions. The plan at
`docs/superpowers/plans/2026-07-21-ash-job-control-hardening.md` implements
**Tier 1 only**.

## Context

`shell/ash.rs` is a c2rust transpile of BusyBox's `ash`, ~15,246 lines / 340
functions: a full lexer, parser, word-expansion engine, executor, job-control
layer, and ~40 builtins. It is the last unaddressed item in MIGRATION.md's
Phase 3 ("no-crate applets... using rustix/nix for syscalls instead of the
transpiled unsafe FFI").

A full from-scratch rewrite was considered and explicitly rejected for this
round, for two concrete reasons discovered during scoping (not just "it's
big"):

1. **`setjmp`/`longjmp` throughout.** ash uses C's `setjmp`/`longjmp` for
   error propagation — e.g. a syntax error deep in the parser jumps straight
   back to the main loop. This doesn't map onto Rust's ownership/Drop model;
   a safe rewrite would need to redesign error propagation entirely (e.g.
   `Result`-based unwinding), which is a genuine architecture decision, not
   a mechanical port.
2. **No usable test coverage.** `testsuite/ash.tests` states outright: *"these
   are not ash tests, we use ash as a way to test lineedit!"* — it only
   checks unicode/line-editing display behavior via a pty. Zero coverage of
   parsing, builtins, control flow, or job control. A rewrite would need its
   own POSIX-conformance test suite built from scratch.

Given that, the chosen direction is **incremental hardening in place**: keep
the existing, working, transpiled control flow (including `setjmp`/`longjmp`
as-is) and replace individual unsafe syscalls with `nix`'s safe wrappers
where doing so is a genuine improvement — not everywhere, and not by
mechanically shimming every extern declaration regardless of whether that
actually reduces unsafety.

## Scope

Initial scoping (grepping the extern declarations' call counts) counted
comment mentions and the declarations themselves as "call sites," inflating
the estimate to ~30. Reading every actual call site during implementation
planning found the real number is smaller (18 real call sites across all 9
functions) — but more importantly, it surfaced that the signal-handling
functions are genuinely entangled with each other and with a function that
was never in scope at all (`sigprocmask`), while the process-group/terminal
functions are cleanly independent. That's the actual reason for the split
below, not just "fewer than expected."

### Tier 1 — in scope, this plan

6 functions, 10 real call sites, all process-group/terminal-control
primitives, fully independent of each other and of anything signal-related:

| function | call sites | why it's in scope |
|---|---|---|
| `fork` | 1 (`forkshell`) | established pattern already (mount.rs/init.rs this session); one small, well-understood conversion (`ForkResult` → 0/pid/-1) |
| `setpgid` | 4 | `nix::unistd::Pid` is a trivial newtype over `pid_t`, no encoding conversion |
| `tcsetpgrp` | 2 | same, via `BorrowedFd::borrow_raw` for the raw fd → `AsFd` bound |
| `tcgetpgrp` | 1 | same |
| `getpgrp` | 1 | same (used alongside the above, inside `setjobctl`) |
| `getppid` | 1 | same (unrelated call site, inside `init()`, sets `$PPID`) |

### Tier 2 — deferred, needs its own separate design pass

Discovered during implementation planning, not during brainstorming — these
looked like the same class of "clean nix swap" as Tier 1 from the outside,
but reading the actual call sites found real entanglement:

- **`sigaction`** (1 real call site, inside `setsignal()`) — it's a
  *query-only* call (`sigaction(signo, NULL, &mut act)`) specifically to
  detect whether the shell inherited `SIG_IGN` for this signal from its
  parent (POSIX-mandated: an interactive/job-control shell must not
  override a signal disposition it inherited as ignored). `nix`'s
  `sigaction()` always performs set-and-return-old in one call; there's no
  clean way to express "query without setting" through its safe API.
- **`signal`** (4 real call sites) — 2 are simple `SIG_DFL` resets
  (`signal(SIGINT, None)` before self-`raise`, and `signal(SIGHUP, None)` at
  shell init) that would convert cleanly, but the other 2 (inside
  `ignoresig()`) share the exact same magic-number `transmute(1) == SIG_IGN`
  pattern `setsignal()` uses. Converting only the easy 2 and leaving the
  other 2 as the same raw hack isn't a real improvement; converting all 4
  needs the `sigaction()` query problem above solved first.
- **`sigfillset` / `sigsuspend`** (2 / 1 real call sites, inside
  `wait_block_or_sig()`) — this function's core logic is an atomic-wait
  idiom (block all signals, then `sigsuspend` in a loop to atomically wait
  for one while briefly unblocking) built from `sigfillset`+`sigsuspend`
  *plus* `sigprocmask`/`crate::libbb::signals::sigprocmask2` — the latter
  two were never in this spec's scope at all. Converting just the two
  scoped functions here would leave a half-converted mix, needing
  back-and-forth conversion between `nix::sys::signal::SigSet` and the raw
  `sigset_t` the neighboring `sigprocmask` calls use, for uncertain benefit.

Tier 2 needs its own scoping pass that includes `sigprocmask` from the
start and works out the inherited-disposition query problem — not
something to guess at inside an implementation plan.

**Out of scope entirely for either tier** (already discovered during
brainstorming, deliberately not touched):

- `pipe` (4 call sites) — `nix::unistd::pipe()` returns owned, auto-closing
  file descriptors, but ash's own fd bookkeeping needs raw fds, so the
  safety benefit is thrown away immediately at the call site. Not worth it
  here; revisit only if ash's fd-tracking itself ever gets rewritten.
- `waitpid` (3 call sites) — nix's rich `WaitStatus` would need converting
  back into the raw wait-status integer encoding ash's own `WIFEXITED`-style
  macros expect elsewhere in the file. Real risk of introducing a subtle
  status-encoding bug for no clear benefit over the direct libc call.
- `killpg`, `raise`, `execve` — not touched this round; no strong safety
  argument either way, left as direct libc calls to keep this increment
  focused.
- The lexer/parser/expansion/executor/builtins — completely untouched. This
  increment only touches process-group/terminal/signal primitives.
- `memcpy`/`memset`/`memcmp`/`memmove`/`strlen` — already fixed by the
  earlier `compat.rs` mechanical pass this session (confirmed: `ash.rs`
  already imports `crate::compat::{memcmp,memcpy,memmove,memset,strlen}`).

## Approach

For each of the 6 Tier 1 functions: read every call site in `ash.rs` to
understand exactly how the return value/output is used, remove the
`extern "C"` declaration, and rewrite each call site directly against nix's
safe API (`Pid`, `ForkResult`, `BorrowedFd::borrow_raw` for the raw-fd →
`AsFd` bound `tcsetpgrp`/`tcgetpgrp` need). No new files or shim module —
this lives entirely within `ash.rs`, since job control is shell-specific and
doesn't belong in the general-purpose `compat.rs` (which exists for
functions with a trivial, universal, same-signature fix; these aren't
that — nix's `Result`-returning API has a different shape than the raw
C return-value convention, so each call site's surrounding code needs a
small, real adaptation, not a drop-in replacement).

The Tier 2 functions (deferred) would need the same direct-rewrite
treatment eventually, but for `sigaction` specifically, preserving the exact
raw `*const libc::sigaction`/`*mut libc::sigaction` call-site signature via
a `compat.rs`-style shim would mean writing code that manually parses/
constructs the C struct fields — real adaptation code that wouldn't
actually be more idiomatic, just relocated unsafety. That's part of why
Tier 2 needs its own design pass rather than a quick follow-on.

## Error handling

Every one of these call sites already goes through `ash.rs`'s existing
error-reporting conventions (mostly `bb_error_msg_and_die`-style calls on
failure, matching the surrounding transpiled code's style). Preserve those
exactly — swap the syscall underneath from raw `libc::` FFI to nix's checked
`Result`, converting `Err` into the same existing failure path each call
site already has. No behavior change on the error path, only on how the
syscall itself is invoked.

## Testing / verification

Same real-verification approach as the rest of Phase 3: build in the Linux
build container (`rustybox-build:latest`), then exercise actual job control
interactively — background a job, `fg`/`bg` it, verify terminal control
transfers correctly via `tcgetpgrp`/`tcsetpgrp`, verify `$PPID` is still
correct. (Tier 1 doesn't touch signal handling, so there's no `SIGTSTP`/
`SIGCONT`/handler-installation verification needed here — that belongs to
whichever pass eventually tackles Tier 2.)

Given `testsuite/ash.tests` has zero real shell-semantics coverage, this
will be manual/scripted verification against real `ash` invocations in the
container (e.g. via `script`/pty or a driven pipe of shell commands), not an
automated `cargo test` addition — job control interacts with the
controlling terminal, the same class of environment-dependent behavior as
`init`'s PID-1 tests (which also couldn't be automated in the shared test
suite, for the same underlying reason: needs a real controlling
terminal/session, not just a subprocess).

## What "done" looks like

- The 6 Tier 1 `extern "C"` declarations (`fork`, `setpgid`, `tcsetpgrp`,
  `tcgetpgrp`, `getpgrp`, `getppid`) are gone from `ash.rs`.
- All 10 call sites compile and behave identically (same error messages,
  same exit codes, same job-control behavior) — verified by hand in the
  build container, not just "it compiles."
- `cargo build --all-features` / the `ash`-inclusive core-features build
  both still succeed at the current 48-warning baseline (this change should
  not introduce new warnings — check the actual "generated N warnings" line
  after, don't assume).
- MIGRATION.md's Phase 3 `ash` bullet is updated to describe this first
  increment and what remains (Tier 2's 4 signal-handling functions plus
  `sigprocmask`, `pipe`/`waitpid`/`killpg`/`raise`/`execve`, and the
  lexer/parser/executor/builtins).

## Explicitly not attempted here

A full ash rewrite, a new test suite, `setjmp`/`longjmp` replacement, the
Tier 2 signal-handling functions, or any change to
parsing/expansion/execution/builtins. This is one bounded, verifiable
increment; the rest of ash stays exactly as it is today.
