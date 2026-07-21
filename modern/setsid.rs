//! `setsid` — Phase 3 idiomatic rewrite (see MIGRATION.md).
//! Runs a program in a new session.

use nix::libc;
use nix::unistd::{execvp, fork, setsid, ForkResult};
use std::ffi::CString;

pub fn run(argv: &[&str]) -> i32 {
  let mut args = argv.iter().skip(1);

  let mut c_flag = false;
  let mut prog_name = args.next();

  if let Some(&"-c") = prog_name {
    c_flag = true;
    prog_name = args.next();
  }

  let Some(prog) = prog_name else {
    eprintln!("setsid: missing PROG");
    return 1;
  };

  // setsid() fails if we are already a process group leader.
  if setsid().is_err() {
    match unsafe { fork() } {
      Ok(ForkResult::Parent { .. }) => return 0,
      Ok(ForkResult::Child) => {
        let _ = setsid();
      }
      Err(e) => {
        eprintln!("setsid: fork failed: {}", e);
        return 1;
      }
    }
  }

  if c_flag {
    // -c: set controlling tty to stdin
    unsafe { libc::ioctl(0, libc::TIOCSCTTY as _, 1) };
  }

  let path = CString::new(*prog).unwrap();
  let mut exec_args = vec![path.clone()];
  for arg in args {
    exec_args.push(CString::new(*arg).unwrap());
  }

  let _ = execvp(&path, &exec_args);
  eprintln!("setsid: can't execute '{}': {}", prog, std::io::Error::last_os_error());
  127
}

pub fn run_and_exit(args: &[&str]) -> ! {
  let code = run(args);
  std::process::exit(code);
}

