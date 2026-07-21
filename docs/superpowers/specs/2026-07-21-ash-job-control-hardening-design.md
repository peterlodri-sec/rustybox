# ash job-control/signal hardening — design

Status: approved, not yet implemented.

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

**In scope** — 9 functions, ~30 call sites total, all process-group/
terminal-control/signal-handling primitives:

| function | call sites | why it's in scope |
|---|---|---|
| `fork` | 4 | established pattern already (mount.rs/init.rs this session); one small, well-understood conversion (`ForkResult` → 0/pid/-1) |
| `setpgid` | 5 | `nix::unistd::Pid` is a trivial newtype over `pid_t`, no encoding conversion |
| `tcsetpgrp` | 3 | same |
| `tcgetpgrp` | 2 | same |
| `getpgrp` | 2 | same (used alongside the above) |
| `getppid` | 2 | same |
| `sigaction` | 4 | signal handling is exactly the class of code that's easy to get subtly wrong in raw FFI; `nix::sys::signal::{SigAction, SigHandler, SigSet, Signal}` map cleanly onto the same kernel disposition |
| `signal` | 6 | same, via `nix::sys::signal::signal` |
| `sigfillset` / `sigsuspend` | 3 / 2 | same, `SigSet` |

**Out of scope for this increment** (already discovered during scoping,
deliberately not touched):

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

For each of the 9 functions: read every call site in `ash.rs` to understand
exactly how the return value/output is used, remove the `extern "C"`
declaration, and rewrite each call site directly against nix's safe API.
No new files or shim module — this lives entirely within `ash.rs`, since job
control is shell-specific and doesn't belong in the general-purpose
`compat.rs` (which exists for functions with a trivial, universal, same-
signature fix; these aren't that).

This is **not** the `compat.rs` "same signature, safe internals" pattern.
For `sigaction` specifically, preserving the exact raw `*const
libc::sigaction`/`*mut libc::sigaction` call-site signature would mean
writing a shim that manually parses/constructs the C struct fields — real
adaptation code that wouldn't actually be more idiomatic, just relocated
unsafety. Rewriting the ~30 call sites directly to use nix's types natively
(`Signal`, `SigAction`, `SigHandler`, `SigSet`, `Pid`) is more real editing
work per call site, but genuinely idiomatic rather than another FFI-shaped
shim.

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
transfers correctly via `tcgetpgrp`, send `SIGTSTP`/`SIGCONT`, verify a
signal handler installed via the new `sigaction` path actually fires.

Given `testsuite/ash.tests` has zero real shell-semantics coverage, this
will be manual/scripted verification against real `ash` invocations in the
container (e.g. via `script`/pty or a driven pipe of shell commands), not an
automated `cargo test` addition — job control interacts with the
controlling terminal, the same class of environment-dependent behavior as
`init`'s PID-1 tests (which also couldn't be automated in the shared test
suite, for the same underlying reason: needs a real controlling
terminal/session, not just a subprocess).

## What "done" looks like

- The 9 targeted `extern "C"` declarations are gone from `ash.rs`.
- Every call site compiles and behaves identically (same error messages,
  same exit codes, same job-control behavior) — verified by hand in the
  build container, not just "it compiles."
- `cargo build --all-features` / the `ash`-inclusive core-features build
  both still succeed at the current 48-warning baseline (this change should
  not introduce new warnings; it may or may not reduce the existing "X
  redeclared with a different signature" warnings for `sigaction`/`signal`/
  etc. if any exist for those symbols — check after, don't assume).
- MIGRATION.md's Phase 3 `ash` bullet is updated to describe this first
  increment and what remains (the lexer/parser/executor/builtins, and the
  five explicitly-deferred syscalls above).

## Explicitly not attempted here

A full ash rewrite, a new test suite, `setjmp`/`longjmp` replacement, or any
change to parsing/expansion/execution/builtins. This is one bounded,
verifiable increment; the rest of ash stays exactly as it is today.
