# ash Job-Control Syscall Hardening (Tier 1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace 6 raw `extern "C"` libc declarations (`fork`, `setpgid`, `tcsetpgrp`, `tcgetpgrp`, `getpgrp`, `getppid`) and their 10 call sites in `shell/ash.rs` with `nix`'s safe wrappers, preserving exact existing behavior.

**Architecture:** No new files. All changes are within `shell/ash.rs`: remove the 6 extern declarations, add `use nix::unistd::{...}` imports, rewrite each call site directly against nix's typed API (`Pid`, `ForkResult`, `Result<_, Errno>`), converting results back into the same C-style values (`0`/`pid`/`-1`, ignored-error `let _ = ...`) the surrounding transpiled code already expects, so no logic outside the touched lines changes.

**Tech Stack:** Rust, `nix` 0.29 (`unistd` feature, already enabled in `Cargo.toml`), the project's Linux Docker build container (`rustybox-build:latest`) for all compilation/verification (macOS host can't compile this Linux-only codebase).

**Context:** This is Tier 1 of a two-tier split discovered during planning (see `docs/superpowers/specs/2026-07-21-ash-job-control-hardening-design.md`). Tier 2 (`sigaction`/`signal`/`sigfillset`/`sigsuspend`, entangled with `sigprocmask`/`sigprocmask2` and inherited-`SIG_IGN` detection) is explicitly deferred to a separate future design pass — do not touch those in this plan.

---

### Task 1: Remove the 6 extern declarations, add nix imports

**Files:**
- Modify: `shell/ash.rs:76-82` (extern block), `shell/ash.rs:1-8` (import block)

- [ ] **Step 1: Remove the 6 target extern declarations**

In `shell/ash.rs`, find this block (currently around line 74-83):

```rust
  fn _exit(_: libc::c_int) -> !;

  fn getppid() -> pid_t;
  fn getpgrp() -> pid_t;
  fn setpgid(__pid: pid_t, __pgid: pid_t) -> libc::c_int;
  fn fork() -> pid_t;

  fn tcgetpgrp(__fd: libc::c_int) -> pid_t;
  fn tcsetpgrp(__fd: libc::c_int, __pgrp_id: pid_t) -> libc::c_int;

  fn _setjmp(_: *mut __jmp_buf_tag) -> libc::c_int;
```

Replace it with (deletes the 6 lines + their now-redundant blank-line separators, keeps `_exit` and `_setjmp` exactly as they were):

```rust
  fn _exit(_: libc::c_int) -> !;

  fn _setjmp(_: *mut __jmp_buf_tag) -> libc::c_int;
```

- [ ] **Step 2: Add the nix imports**

Find the top of `shell/ash.rs` (currently starting):

```rust
use crate::libbb::ptr_to_globals::bb_errno;
use crate::libbb::xfuncs_printf::xmalloc;
```

Replace with (adds two new `use` lines immediately before the existing ones — nothing else in this range changes):

```rust
use nix::unistd::{fork, getpgrp, getppid, setpgid, tcgetpgrp, tcsetpgrp, ForkResult, Pid};
use std::os::fd::BorrowedFd;

use crate::libbb::ptr_to_globals::bb_errno;
use crate::libbb::xfuncs_printf::xmalloc;
```

- [ ] **Step 3: Confirm this doesn't compile yet (expected — call sites still use the old convention)**

Run: `docker run --rm -v "$PWD":/work -v rustybox-cargo-registry:/usr/local/cargo/registry -v rustybox-target:/work/target rustybox-build:latest cargo check --all-features 2>&1 | grep -E "^error" | head -20`

Expected: several `error[E0308]` (mismatched types) at the 10 call sites touched by later tasks in this plan — e.g. `expected i32, found Result<...>` for the `fork()` call, `expected i32, found Result<(), Errno>` for `setpgid`/`tcsetpgrp` calls, `expected i32, found Pid` for `getpgrp`/`getppid`. This is expected and will be resolved by Tasks 2-7. Do NOT proceed to fix these here — each subsequent task fixes its own call sites.

- [ ] **Step 4: Commit**

```bash
git add shell/ash.rs
git commit -m "refactor(ash): remove extern decls for fork/setpgid/tc{s,g}etpgrp/getp{grp,pid}, add nix imports (call sites fixed in following commits)"
```

---

### Task 2: Convert the `fork()` call site

**Files:**
- Modify: `shell/ash.rs` (inside `forkshell`, currently around line 4434)

- [ ] **Step 1: Rewrite the call site**

Find (inside `unsafe extern "C" fn forkshell`):

```rust
  let mut pid: libc::c_int = 0;
  pid = fork();
  if pid < 0 {
```

Replace with:

```rust
  let mut pid: libc::c_int = 0;
  pid = match fork() {
    Ok(ForkResult::Child) => 0,
    Ok(ForkResult::Parent { child }) => child.as_raw(),
    Err(_) => -1,
  };
  if pid < 0 {
```

- [ ] **Step 2: Commit**

```bash
git add shell/ash.rs
git commit -m "refactor(ash): forkshell uses nix::unistd::fork"
```

---

### Task 3: Convert the 4 `setpgid` call sites

**Files:**
- Modify: `shell/ash.rs` (inside `setjobctl` x2, `forkchild`, `forkparent`)

- [ ] **Step 1: Fix the `setjobctl` on-branch call site**

Find:

```rust
              pgrp = (*ash_ptr_to_globals_misc).rootpid;
              setpgid(0i32, pgrp);
              xtcsetpgrp(fd, pgrp);
```

Replace with:

```rust
              pgrp = (*ash_ptr_to_globals_misc).rootpid;
              let _ = setpgid(Pid::from_raw(0), Pid::from_raw(pgrp));
              xtcsetpgrp(fd, pgrp);
```

- [ ] **Step 2: Fix the `setjobctl` off-branch call site**

Find:

```rust
    tcsetpgrp(fd, pgrp);
    setpgid(0i32, pgrp);
    setsignal(20i32);
```

Replace with:

```rust
    tcsetpgrp(fd, pgrp);
    let _ = setpgid(Pid::from_raw(0), Pid::from_raw(pgrp));
    setsignal(20i32);
```

(The `tcsetpgrp(fd, pgrp);` line here is fixed in Task 4 — leave it as-is in this step, it will still fail to compile until Task 4 runs. That's expected within this plan's task ordering.)

- [ ] **Step 3: Fix the `forkchild` call site**

Find:

```rust
    setpgid(0i32, pgrp);
    if mode == 0 {
      xtcsetpgrp(ttyfd, pgrp);
    }
```

Replace with:

```rust
    let _ = setpgid(Pid::from_raw(0), Pid::from_raw(pgrp));
    if mode == 0 {
      xtcsetpgrp(ttyfd, pgrp);
    }
```

- [ ] **Step 4: Fix the `forkparent` call site**

Find:

```rust
    setpgid(pid, pgrp);
  }
  if mode == 1i32 {
```

Replace with:

```rust
    let _ = setpgid(Pid::from_raw(pid), Pid::from_raw(pgrp));
  }
  if mode == 1i32 {
```

- [ ] **Step 5: Commit**

```bash
git add shell/ash.rs
git commit -m "refactor(ash): setjobctl/forkchild/forkparent use nix::unistd::setpgid"
```

---

### Task 4: Convert the 2 `tcsetpgrp` call sites

**Files:**
- Modify: `shell/ash.rs` (`xtcsetpgrp`, `setjobctl` off-branch)

- [ ] **Step 1: Fix `xtcsetpgrp` (has a return-value check)**

Find:

```rust
unsafe extern "C" fn xtcsetpgrp(mut fd: libc::c_int, mut pgrp: pid_t) {
  if tcsetpgrp(fd, pgrp) != 0 {
    ash_msg_and_raise_error(
      b"can\'t set tty process group: %m\x00" as *const u8 as *const libc::c_char,
    );
  };
}
```

Replace with:

```rust
unsafe extern "C" fn xtcsetpgrp(mut fd: libc::c_int, mut pgrp: pid_t) {
  if tcsetpgrp(BorrowedFd::borrow_raw(fd), Pid::from_raw(pgrp)).is_err() {
    ash_msg_and_raise_error(
      b"can\'t set tty process group: %m\x00" as *const u8 as *const libc::c_char,
    );
  };
}
```

- [ ] **Step 2: Fix the `setjobctl` off-branch call site (no return-value check, matches upstream — errors here were silently ignored before too)**

Find:

```rust
    tcsetpgrp(fd, pgrp);
    let _ = setpgid(Pid::from_raw(0), Pid::from_raw(pgrp));
```

Replace with:

```rust
    let _ = tcsetpgrp(BorrowedFd::borrow_raw(fd), Pid::from_raw(pgrp));
    let _ = setpgid(Pid::from_raw(0), Pid::from_raw(pgrp));
```

- [ ] **Step 3: Commit**

```bash
git add shell/ash.rs
git commit -m "refactor(ash): xtcsetpgrp/setjobctl use nix::unistd::tcsetpgrp"
```

---

### Task 5: Convert the `tcgetpgrp` call site

**Files:**
- Modify: `shell/ash.rs` (inside `setjobctl`, currently around line 3153)

- [ ] **Step 1: Rewrite the call site**

Find:

```rust
          loop {
            pgrp = tcgetpgrp(fd);
            if pgrp < 0 {
              current_block = 14414541239968212827;
              break;
            }
```

Replace with:

```rust
          loop {
            pgrp = match tcgetpgrp(BorrowedFd::borrow_raw(fd)) {
              Ok(p) => p.as_raw(),
              Err(_) => -1,
            };
            if pgrp < 0 {
              current_block = 14414541239968212827;
              break;
            }
```

- [ ] **Step 2: Commit**

```bash
git add shell/ash.rs
git commit -m "refactor(ash): setjobctl uses nix::unistd::tcgetpgrp"
```

---

### Task 6: Convert the `getpgrp` call site

**Files:**
- Modify: `shell/ash.rs` (inside `setjobctl`, currently around line 3158)

- [ ] **Step 1: Rewrite the call site**

Find:

```rust
            if pgrp == getpgrp() {
              initialpgrp = pgrp;
```

Replace with:

```rust
            if pgrp == getpgrp().as_raw() {
              initialpgrp = pgrp;
```

- [ ] **Step 2: Commit**

```bash
git add shell/ash.rs
git commit -m "refactor(ash): setjobctl uses nix::unistd::getpgrp"
```

---

### Task 7: Convert the `getppid` call site

**Files:**
- Modify: `shell/ash.rs` (inside `init`, currently around line 14432)

- [ ] **Step 1: Rewrite the call site**

Find:

```rust
  setvar0(
    b"PPID\x00" as *const u8 as *const libc::c_char,
    crate::libbb::xfuncs::utoa(getppid() as libc::c_uint),
  );
```

Replace with:

```rust
  setvar0(
    b"PPID\x00" as *const u8 as *const libc::c_char,
    crate::libbb::xfuncs::utoa(getppid().as_raw() as libc::c_uint),
  );
```

- [ ] **Step 2: Commit**

```bash
git add shell/ash.rs
git commit -m "refactor(ash): init() uses nix::unistd::getppid"
```

---

### Task 8: Full compile verification

**Files:** none (verification only)

- [ ] **Step 1: `cargo check --all-features` — expect zero errors**

Run:
```bash
docker run --rm -v "$PWD":/work -v rustybox-cargo-registry:/usr/local/cargo/registry -v rustybox-target:/work/target rustybox-build:latest cargo check --all-features > /tmp/ash_hardening_check.log 2>&1
echo "exit=$?"
grep -c '^error' /tmp/ash_hardening_check.log
```
Expected: `exit=0`, and the error count is `0` (grep with no matches returns exit 1, which is fine — the point is zero lines starting with `error`).

- [ ] **Step 2: Confirm no new warnings from the touched lines**

Run:
```bash
docker run --rm -v "$PWD":/work -v rustybox-cargo-registry:/usr/local/cargo/registry -v rustybox-target:/work/target rustybox-build:latest bash -c "cargo build --all-features 2>&1 | tail -3"
```
Expected: warning count stays at the 48-warning baseline (do not assume — read the actual "generated N warnings" line and compare to 48; if it changed, investigate why before proceeding, don't just note it and move on).

- [ ] **Step 3: Build the actual binary with `ash` enabled (matches `CORE_FEATURES` in `docker/build.sh`)**

Run:
```bash
docker run --rm -v "$PWD":/work -v rustybox-cargo-registry:/usr/local/cargo/registry -v rustybox-target:/work/target rustybox-build:latest bash -c "cargo build --no-default-features --features 'cat ls echo cp mv rm mkdir rmdir ln pwd touch true false head tail wc sort uniq grep sed cut tr chmod chown df du ps kill sleep env printenv date basename dirname readlink stat which id whoami ash' 2>&1 | tail -5"
```
Expected: `Finished` with no errors.

---

### Task 9: Manual job-control behavior verification

**Files:** none (verification only — see the design spec's "Testing / verification" section for why this can't be automated: `testsuite/ash.tests` has zero real shell-semantics coverage, and job control needs a real controlling terminal, the same class of environment-dependent behavior `init`'s PID-1 tests hit earlier this session)

- [ ] **Step 1: Build the debug binary and set up an `ash` symlink in the container**

```bash
docker run --rm -it \
  -v "$PWD":/work \
  -v rustybox-cargo-registry:/usr/local/cargo/registry \
  -v rustybox-target:/work/target \
  rustybox-build:latest bash
```
Inside the container:
```bash
cargo build --no-default-features --features "ash cat echo" 2>&1 | tail -3
cp target/debug/rustybox /tmp/rb
ln -sf /tmp/rb /tmp/ash
```

- [ ] **Step 2: Verify basic job control (background, jobs, fg) via a scripted `ash` session**

Still inside the container:
```bash
/tmp/ash -c '
sleep 30 &
jobs
fg %1
' &
BGPID=$!
sleep 1
echo "--- jobs output should show sleep as job 1 ---"
sleep 2
kill -9 $BGPID 2>/dev/null
```
Expected: no crash, `jobs` prints a line referencing `sleep`, no panic/segfault. (This is a smoke test for "did the `setpgid`/`tcgetpgrp` changes break job creation" — full interactive fg semantics need a real pty, tested next.)

- [ ] **Step 3: Verify interactive job control with a real pty via `script`**

Still inside the container (this needs a real controlling terminal, hence `script`):
```bash
script -qc '/tmp/ash' /tmp/ash_session.log <<'EOF'
sleep 100 &
jobs
fg
EOF
sleep 1
cat /tmp/ash_session.log
```
Expected: the log shows the backgrounded `sleep 100` job listed by `jobs`, and `fg` bringing it to the foreground without an error message like "can't set tty process group" (that specific error string comes directly from the `xtcsetpgrp` failure path touched in Task 4 — seeing it here would mean the `tcsetpgrp` conversion introduced a regression).

- [ ] **Step 4: Verify `$PPID` still works (getppid conversion)**

```bash
/tmp/ash -c 'echo $PPID'
```
Expected: prints a numeric PID (the shell's own parent, i.e. this docker/bash session's pid) — not `0`, not an error, not garbage.

- [ ] **Step 5: If anything in Steps 2-4 fails or behaves differently than before this plan's changes**

Do not proceed to Task 10. Go back to the specific task (2-7) responsible for the failing behavior, re-read the exact original call site in git history (`git log -p -- shell/ash.rs` around the relevant commit), and fix the conversion — the goal is byte-for-byte identical behavior, not "close enough."

---

### Task 10: Update MIGRATION.md and finalize

**Files:**
- Modify: `MIGRATION.md`

- [ ] **Step 1: Update the `ash` bullet in MIGRATION.md's Phase 3 section**

Find (the `ash` bullet added at the end of the Phase 3 list):

```markdown
  - `ash` — not yet started; 15k transpiled lines, full POSIX shell
    semantics, a multi-session project on its own, not a quick swap.
```

Replace with:

```markdown
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
```

- [ ] **Step 2: Commit**

```bash
git add MIGRATION.md
git commit -m "docs: update MIGRATION.md for ash Tier 1 job-control hardening"
```

- [ ] **Step 3: Push**

```bash
git push
```

- [ ] **Step 4: Verify the push succeeded and CI is green**

```bash
git log --oneline -8
gh run list --branch master --limit 3
```

Expected: the 8 commits from this plan appear at the top of `git log`, and the most recent CI run for master either succeeds or is still in progress (not already failed — if failed, investigate before considering this plan complete).
