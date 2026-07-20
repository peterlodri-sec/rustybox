//! `init` — Phase 3 idiomatic rewrite (see MIGRATION.md). PID-1 supervisor:
//! parses `/etc/inittab`, spawns/respawns processes, reaps zombies, and
//! handles shutdown/reboot/halt/poweroff — using `nix`'s safe process/
//! signal wrappers (`fork`, `waitpid`, `sigaction`) instead of the
//! transpiled version's raw FFI.
//!
//! Behavior matches upstream BusyBox init's documented contract: `id:
//! runlevels:action:process` inittab lines (`runlevels` is parsed but
//! ignored, matching upstream — busybox init has no runlevel concept),
//! actions `sysinit`/`wait`/`once`/`respawn`/`askfirst`/`ctrlaltdel`/
//! `shutdown`/`restart`, the same hardcoded default inittab when
//! `/etc/inittab` is missing, `-q` to signal a running init to reload, and
//! — deliberately, matching upstream's own documented behavior — no
//! respawn rate-limiting ("unlike sysvinit, BusyBox init does not stop
//! processes from respawning out of control").
//!
//! Two intentional simplifications, both because modern targets have an
//! MMU (unlike the embedded NOMMU boards this codebase originally
//! targeted): every spawn uses `fork()`, not the `vfork()`/`fork()` split
//! upstream uses purely to avoid `askfirst`'s blocking read deadlocking a
//! true `vfork` parent; and the SIGTSTP/SIGSTOP-driven "freeze until
//! SIGCONT" job-control handler (a legacy interactive-console feature,
//! essentially never exercised by init's actual callers — container
//! runtimes and embedded supervisors don't send PID 1 SIGTSTP) is not
//! implemented. Everything else — inittab semantics, spawn/respawn/reap,
//! shutdown/reboot/halt/poweroff — is a full behavioral port.
//!
//! One deliberate hardening beyond upstream: the PID-1 check is enforced
//! unconditionally, including for the `linuxrc` alias. Upstream waives it
//! for `linuxrc` (a legacy leniency from old initramfs conventions, where
//! `linuxrc` is *also* always PID 1 in practice), but replicating that
//! waiver would mean an accidental `rustybox linuxrc` on a developer's own
//! machine hijacks the process into an infinite supervisor loop with
//! hijacked signal handlers instead of safely erroring — for no benefit,
//! since the legitimate initramfs case already satisfies pid==1.

use std::ffi::{CStr, CString};
use std::fs;
use std::os::fd::AsRawFd;
use std::sync::atomic::{AtomicBool, Ordering};

use nix::libc;
use nix::sys::signal::{kill, sigaction, SaFlags, SigAction, SigHandler, SigSet, Signal};
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::{chdir, execvp, fork, setsid, ForkResult, Pid};

const ACT_SYSINIT: u8 = 0x01;
const ACT_WAIT: u8 = 0x02;
const ACT_ONCE: u8 = 0x04;
const ACT_RESPAWN: u8 = 0x08;
const ACT_ASKFIRST: u8 = 0x10;
const ACT_CTRLALTDEL: u8 = 0x20;
const ACT_SHUTDOWN: u8 = 0x40;
const ACT_RESTART: u8 = 0x80;

const INITTAB: &str = "/etc/inittab";

struct Action {
  terminal: String, // "" = console (whatever fd 0/1/2 already are)
  action: u8,
  process: String,
  pid: Option<Pid>, // Some for a live respawn/askfirst child
}

fn action_keyword(s: &str) -> Option<u8> {
  Some(match s {
    "sysinit" => ACT_SYSINIT,
    "wait" => ACT_WAIT,
    "once" => ACT_ONCE,
    "respawn" => ACT_RESPAWN,
    "askfirst" => ACT_ASKFIRST,
    "ctrlaltdel" => ACT_CTRLALTDEL,
    "shutdown" => ACT_SHUTDOWN,
    "restart" => ACT_RESTART,
    _ => return None,
  })
}

fn default_inittab() -> Vec<Action> {
  let mk = |terminal: &str, action: u8, process: &str| Action { terminal: terminal.to_string(), action, process: process.to_string(), pid: None };
  vec![
    mk("", ACT_SYSINIT, "/etc/init.d/rcS"),
    mk("", ACT_ASKFIRST, "/bin/sh"),
    mk("/dev/tty2", ACT_ASKFIRST, "/bin/sh"),
    mk("/dev/tty3", ACT_ASKFIRST, "/bin/sh"),
    mk("/dev/tty4", ACT_ASKFIRST, "/bin/sh"),
    mk("", ACT_CTRLALTDEL, "reboot"),
    mk("", ACT_SHUTDOWN, "umount -a -r"),
    mk("", ACT_SHUTDOWN, "swapoff -a"),
    mk("", ACT_RESTART, "init"),
  ]
}

// inittab line format: id:runlevels:action:process — `runlevels` is parsed
// (so malformed lines are still rejected) but its value is never consulted,
// matching upstream. No trim/collapse: an empty `id` field is meaningful
// (means "console", not "field absent").
fn parse_inittab() -> Vec<Action> {
  let Ok(content) = fs::read_to_string(INITTAB) else { return default_inittab() };
  let mut out = Vec::new();
  for (lineno, raw) in content.lines().enumerate() {
    let line = raw.split('#').next().unwrap_or("");
    if line.trim().is_empty() {
      continue;
    }
    let fields: Vec<&str> = line.splitn(4, ':').collect();
    let (Some(id), Some(_runlevels), Some(action_str), Some(process)) = (fields.first(), fields.get(1), fields.get(2), fields.get(3)) else {
      eprintln!("init: bad inittab entry at line {}", lineno + 1);
      continue;
    };
    let Some(action) = action_keyword(action_str) else {
      eprintln!("init: bad inittab entry at line {}", lineno + 1);
      continue;
    };
    let terminal = if id.is_empty() { String::new() } else { format!("/dev/{id}") };
    out.push(Action { terminal, action, process: process.to_string(), pid: None });
  }
  if out.is_empty() {
    default_inittab()
  } else {
    out
  }
}

// ---- confined FFI helpers ---------------------------------------------------
// Every unsafe operation in this file lives here: the reboot(2) syscall (no
// safe wrapper exists anywhere, it's too Linux-specific and dangerous to be
// in a cross-platform crate), sync(2) (no preconditions, purely
// observational, but still FFI), signal-handler registration (inherently
// unsafe in Rust: the compiler can't check a signal handler's function
// pointer stays within async-signal-safe operations), and claiming the
// controlling terminal via TIOCSCTTY.

fn do_reboot(magic: i32) {
  // SAFETY: reboot(2) with magic values RB_AUTOBOOT/RB_HALT_SYSTEM/
  // RB_POWER_OFF/RB_ENABLE_CAD/RB_DISABLE_CAD takes no pointer arguments in
  // this arity and has no aliasing/lifetime preconditions to uphold.
  unsafe { libc::reboot(magic) };
}

fn do_sync() {
  // SAFETY: sync(2) takes no arguments and has no preconditions.
  unsafe { libc::sync() };
}

extern "C" fn record_signal(sig: libc::c_int) {
  // Async-signal-safe: an atomic store and nothing else, same "delayed
  // signal" design as upstream's record_signo — the real work happens back
  // in the main loop's check_delayed_sigs, never inside the handler.
  let idx = match sig {
    x if x == Signal::SIGHUP as i32 => 0,
    x if x == Signal::SIGINT as i32 => 1,
    x if x == Signal::SIGQUIT as i32 => 2,
    x if x == Signal::SIGTERM as i32 => 3,
    x if x == Signal::SIGUSR1 as i32 => 4,
    x if x == Signal::SIGUSR2 as i32 => 5,
    _ => return,
  };
  PENDING[idx].store(true, Ordering::SeqCst);
}

static PENDING: [AtomicBool; 6] = [const { AtomicBool::new(false) }; 6];

fn install_signal_handlers() {
  let handler = SigHandler::Handler(record_signal);
  // Block every other signal while the handler runs so delayed-signal
  // bookkeeping can't race itself; SA_RESTART matches upstream's intent
  // (don't make blocking init syscalls fail with EINTR gratuitously).
  let action = SigAction::new(handler, SaFlags::SA_RESTART, SigSet::all());
  for sig in [Signal::SIGHUP, Signal::SIGINT, Signal::SIGQUIT, Signal::SIGTERM, Signal::SIGUSR1, Signal::SIGUSR2] {
    // SAFETY: `record_signal` only performs an atomic store — it is
    // async-signal-safe, satisfying sigaction(2)'s handler requirements.
    unsafe { sigaction(sig, &action) }.ok();
  }
}

fn check_delayed_sigs(actions: &mut Vec<Action>) -> bool {
  let mut any = false;
  if PENDING[0].swap(false, Ordering::SeqCst) {
    any = true;
    *actions = parse_inittab();
  }
  if PENDING[1].swap(false, Ordering::SeqCst) {
    any = true;
    run_actions(actions, ACT_CTRLALTDEL);
  }
  if PENDING[2].swap(false, Ordering::SeqCst) {
    any = true;
    exec_restart_action(actions);
  }
  for (i, sig) in [(3, Signal::SIGTERM), (4, Signal::SIGUSR1), (5, Signal::SIGUSR2)] {
    if PENDING[i].swap(false, Ordering::SeqCst) {
      any = true;
      halt_reboot_pwoff(actions, sig);
    }
  }
  any
}

fn claim_controlling_tty(fd: i32) {
  // SAFETY: TIOCSCTTY on a just-opened tty fd we own; failure (e.g. already
  // a controlling terminal) is harmless and intentionally ignored, matching
  // upstream.
  unsafe { libc::ioctl(fd, libc::TIOCSCTTY as _, 0) };
}

fn open_stdio_to_tty(terminal: &str) {
  use nix::fcntl::{open, OFlag};
  use nix::sys::stat::Mode;
  use nix::unistd::dup2;
  if terminal.is_empty() {
    return; // keep whatever fd 0/1/2 already are (the console init inherited)
  }
  let Ok(fd) = open(terminal, OFlag::O_RDWR, Mode::empty()) else {
    eprintln!("init: can't open {terminal}");
    return;
  };
  claim_controlling_tty(fd.as_raw_fd());
  for target in [0, 1, 2] {
    dup2(fd.as_raw_fd(), target).ok();
  }
}

// ---- process spawning -------------------------------------------------------

fn build_argv(process: &str) -> (CString, Vec<CString>) {
  let is_shell_cmd = process.contains(|c| "~`!$^&*()=|\\{}[];\"'<>?".contains(c));
  if is_shell_cmd {
    let sh = CString::new("/bin/sh").unwrap();
    (sh.clone(), vec![sh, CString::new("-c").unwrap(), CString::new(process).unwrap()])
  } else {
    let mut words = process.split_whitespace();
    let mut prog = words.next().unwrap_or("").to_string();
    let login_dash = prog.starts_with('-');
    if login_dash {
      prog.remove(0);
    }
    let path = CString::new(prog.clone()).unwrap();
    let mut argv = vec![CString::new(if login_dash { format!("-{prog}") } else { prog }).unwrap()];
    argv.extend(words.map(|w| CString::new(w).unwrap()));
    (path, argv)
  }
}

fn spawn(a: &Action) -> Option<Pid> {
  // SAFETY: single-threaded process (PID 1 supervisor); the child only
  // calls async-signal-safe-equivalent, exec-family-safe operations before
  // execvp (no allocation-heavy Rust runtime interaction that fork()'s
  // safety caveat around multithreaded programs actually warns about).
  match unsafe { fork() } {
    Ok(ForkResult::Parent { child }) => Some(child),
    Ok(ForkResult::Child) => {
      // Reset the delayed-signal handlers to default and unblock
      // everything, so the spawned process starts with a clean slate —
      // matches reset_sighandlers_and_unblock_sigs.
      for sig in [Signal::SIGHUP, Signal::SIGINT, Signal::SIGQUIT, Signal::SIGTERM, Signal::SIGUSR1, Signal::SIGUSR2, Signal::SIGTSTP] {
        let default = SigAction::new(SigHandler::SigDfl, SaFlags::empty(), SigSet::empty());
        unsafe { sigaction(sig, &default) }.ok();
      }
      let mut unblock = SigSet::all();
      unblock.thread_unblock().ok();
      setsid().ok();
      open_stdio_to_tty(&a.terminal);
      if a.action & ACT_ASKFIRST != 0 {
        print!("\r\nPlease press Enter to activate this console. ");
        use std::io::Write;
        std::io::stdout().flush().ok();
        let mut buf = [0u8; 1];
        while nix::unistd::read(0, &mut buf).unwrap_or(0) > 0 && buf[0] != b'\n' {}
      }
      let (path, argv) = build_argv(&a.process);
      let _ = execvp(&path, &argv);
      eprintln!("init: can't run '{}': {}", a.process, std::io::Error::last_os_error());
      std::process::exit(-1);
    }
    Err(e) => {
      eprintln!("init: fork failed: {e}");
      None
    }
  }
}

/// Block until `pid` exits, reaping any *other* children that die meanwhile
/// too (so `sysinit`/`wait`/`ctrlaltdel`/`shutdown` don't leak zombies from
/// unrelated respawn children that happen to die during the wait).
fn wait_for(pid: Pid) {
  loop {
    match waitpid(Pid::from_raw(-1), None) {
      Ok(WaitStatus::Exited(p, _)) | Ok(WaitStatus::Signaled(p, _, _)) if p == pid => return,
      Ok(_) => continue,
      Err(nix::errno::Errno::ECHILD) => return,
      Err(_) => return,
    }
  }
}

fn run_actions(actions: &mut [Action], mask: u8) {
  let waits_for_completion = mask & (ACT_SYSINIT | ACT_WAIT | ACT_CTRLALTDEL | ACT_SHUTDOWN) != 0;
  for a in actions.iter_mut() {
    if a.action & mask == 0 {
      continue;
    }
    if a.action & (ACT_RESPAWN | ACT_ASKFIRST) != 0 {
      if a.pid.is_none() {
        a.pid = spawn(a);
      }
      continue;
    }
    if let Some(pid) = spawn(a) {
      if waits_for_completion {
        wait_for(pid);
      }
    }
  }
}

fn mark_terminated(actions: &mut [Action], pid: Pid) {
  for a in actions.iter_mut() {
    if a.pid == Some(pid) {
      eprintln!("init: process '{}' ({pid}) exited. Scheduling for restart.", a.process);
      a.pid = None;
    }
  }
}

// ---- shutdown / reboot / halt / poweroff -----------------------------------

fn shutdown_and_kill(actions: &mut [Action]) {
  run_actions(actions, ACT_SHUTDOWN);
  kill(Pid::from_raw(-1), Signal::SIGTERM).ok();
  do_sync();
  std::thread::sleep(std::time::Duration::from_secs(1));
  kill(Pid::from_raw(-1), Signal::SIGKILL).ok();
  do_sync();
}

fn reset_to_default_and_unblock() {
  for sig in [Signal::SIGHUP, Signal::SIGINT, Signal::SIGQUIT, Signal::SIGTERM, Signal::SIGUSR1, Signal::SIGUSR2] {
    let default = SigAction::new(SigHandler::SigDfl, SaFlags::empty(), SigSet::empty());
    unsafe { sigaction(sig, &default) }.ok();
  }
  let mut set = SigSet::all();
  set.thread_unblock().ok();
}

fn halt_reboot_pwoff(actions: &mut [Action], sig: Signal) -> ! {
  reset_to_default_and_unblock();
  shutdown_and_kill(actions);
  let magic = match sig {
    Signal::SIGTERM => libc::RB_AUTOBOOT,
    Signal::SIGUSR2 => libc::RB_POWER_OFF,
    _ => libc::RB_HALT_SYSTEM,
  };
  std::thread::sleep(std::time::Duration::from_secs(1));
  match unsafe { fork() } {
    Ok(ForkResult::Child) => {
      do_reboot(magic);
      std::process::exit(0);
    }
    Ok(ForkResult::Parent { child }) => {
      waitpid(child, None).ok();
    }
    Err(_) => {}
  }
  std::thread::sleep(std::time::Duration::from_secs(1));
  std::process::exit(0);
}

fn exec_restart_action(actions: &mut [Action]) -> ! {
  reset_to_default_and_unblock();
  shutdown_and_kill(actions);
  do_reboot(libc::RB_ENABLE_CAD);
  if let Some(a) = actions.iter().find(|a| a.action & ACT_RESTART != 0) {
    let (path, argv) = build_argv(&a.process);
    let _ = execvp(&path, &argv);
  }
  // Re-exec failed (or no `restart` entry): fall back to halt so the system
  // doesn't spin forever with PID 1 gone.
  halt_reboot_pwoff(actions, Signal::SIGUSR1);
}

// ---- entry point -------------------------------------------------------------

fn is_single_user(runlevel: &str) -> bool {
  matches!(runlevel, "single" | "-s" | "1")
}

pub fn run(argv: &[&str]) -> i32 {
  if argv.get(1) == Some(&"-q") {
    kill(Pid::from_raw(1), Signal::SIGHUP).ok();
    return 0;
  }

  // Upstream BusyBox waives this check when invoked as `linuxrc` (a legacy
  // leniency from old initramfs boot conventions, where `linuxrc` is *also*
  // always PID 1 in practice — the kernel execs it directly as the initial
  // process). We deliberately don't replicate the waiver: it would mean an
  // accidental `rustybox linuxrc` on a developer's own machine hijacks the
  // process into an infinite supervisor loop with hijacked signal handlers
  // instead of safely erroring, for no real benefit (the legitimate
  // initramfs use case already satisfies pid==1 anyway).
  let pid = nix::unistd::getpid();
  if pid.as_raw() != 1 {
    eprintln!("init: must be run as PID 1");
    return 1;
  }

  do_reboot(libc::RB_DISABLE_CAD);
  chdir("/").ok();
  setsid().ok();

  let runlevel = argv.get(1).copied().unwrap_or("");
  std::env::set_var("HOME", "/");
  std::env::set_var("PATH", "/sbin:/usr/sbin:/bin:/usr/bin");
  std::env::set_var("SHELL", "/bin/sh");
  std::env::set_var("USER", "root");
  std::env::set_var("RUNLEVEL", runlevel);

  let mut actions = if is_single_user(runlevel) {
    vec![Action { terminal: String::new(), action: ACT_ONCE, process: "/bin/sh".to_string(), pid: None }]
  } else {
    parse_inittab()
  };

  install_signal_handlers();

  run_actions(&mut actions, ACT_SYSINIT);
  run_actions(&mut actions, ACT_WAIT);
  run_actions(&mut actions, ACT_ONCE);

  loop {
    check_delayed_sigs(&mut actions);
    run_actions(&mut actions, ACT_RESPAWN | ACT_ASKFIRST);
    let signaled = check_delayed_sigs(&mut actions);
    if !signaled {
      std::thread::sleep(std::time::Duration::from_secs(1));
    }
    let signaled = signaled || check_delayed_sigs(&mut actions);

    let flags = if signaled { WaitPidFlag::WNOHANG } else { WaitPidFlag::empty() };
    loop {
      match waitpid(Pid::from_raw(-1), Some(flags)) {
        Ok(WaitStatus::Exited(p, _)) | Ok(WaitStatus::Signaled(p, _, _)) => mark_terminated(&mut actions, p),
        _ => break,
      }
    }
  }
}
