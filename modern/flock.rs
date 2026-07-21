//! `flock` — Phase 3 idiomatic rewrite (see MIGRATION.md).
//! Manage locks from shell scripts.

use nix::libc;
use nix::unistd::{execvp, fork, ForkResult};
use nix::sys::wait::{waitpid, WaitStatus};
use std::ffi::CString;

pub fn run(argv: &[&str]) -> i32 {
  let mut shared = false;
  let mut exclusive = false;
  let mut unlock = false;
  let mut nonblock = false;

  let mut idx = 1;
  while idx < argv.len() {
    let arg = argv[idx];
    if arg.starts_with('-') && arg != "-" {
      if arg == "--shared" { shared = true; }
      else if arg == "--exclusive" { exclusive = true; }
      else if arg == "--unlock" { unlock = true; }
      else if arg == "--nonblock" { nonblock = true; }
      else if arg == "--help" { return 0; }
      else {
        for c in arg.chars().skip(1) {
          match c {
            's' => shared = true,
            'x' => exclusive = true,
            'u' => unlock = true,
            'n' => nonblock = true,
            _ => {
              eprintln!("flock: invalid option -- '{}'", c);
              return 1;
            }
          }
        }
      }
      idx += 1;
    } else {
      break;
    }
  }

  if idx >= argv.len() {
    eprintln!("flock: missing operand");
    return 1;
  }

  let file_or_fd = argv[idx];
  idx += 1;

  let mut command_mode = false;
  if idx < argv.len() {
    let arg = argv[idx];
    if arg == "-c" || arg == "--command" {
      command_mode = true;
      idx += 1;
    }
  }

  let cmd_args = &argv[idx..];

  let mut fd = -1;
  if cmd_args.is_empty() {
    // Treat as FD
    if let Ok(n) = file_or_fd.parse::<i32>() {
      if n >= 0 {
        fd = n;
      }
    }
    if fd < 0 {
      eprintln!("flock: bad file descriptor");
      return 1;
    }
  } else {
    // Treat as FILE
    let path = CString::new(file_or_fd).unwrap();
    fd = unsafe { libc::open(path.as_ptr(), libc::O_RDONLY | libc::O_NOCTTY | libc::O_CREAT, 0o666) };
    if fd < 0 && std::io::Error::last_os_error().raw_os_error() == Some(libc::EISDIR) {
      fd = unsafe { libc::open(path.as_ptr(), libc::O_RDONLY | libc::O_NOCTTY) };
    }
    if fd < 0 {
      eprintln!("flock: can't open '{}': {}", file_or_fd, std::io::Error::last_os_error());
      return 1;
    }
  }

  let mut mode = if unlock {
    libc::LOCK_UN
  } else if shared {
    libc::LOCK_SH
  } else {
    libc::LOCK_EX
  };
  if nonblock {
    mode |= libc::LOCK_NB;
  }

  if unsafe { libc::flock(fd, mode) } != 0 {
    let err = std::io::Error::last_os_error();
    if err.raw_os_error() == Some(libc::EWOULDBLOCK) || err.raw_os_error() == Some(libc::EAGAIN) {
      return 1;
    }
    eprintln!("flock: {}", err);
    return 1;
  }

  if !cmd_args.is_empty() {
    match unsafe { fork() } {
      Ok(ForkResult::Parent { child }) => {
        match waitpid(child, None) {
          Ok(WaitStatus::Exited(_, code)) => return code,
          Ok(WaitStatus::Signaled(_, sig, _)) => return sig as i32 + 128,
          _ => return 1,
        }
      }
      Ok(ForkResult::Child) => {
        let mut exec_argv = Vec::new();
        let prog;
        if command_mode {
          prog = CString::new("/bin/sh").unwrap();
          exec_argv.push(prog.clone());
          exec_argv.push(CString::new("-c").unwrap());
          for arg in cmd_args {
            exec_argv.push(CString::new(*arg).unwrap());
          }
        } else {
          prog = CString::new(cmd_args[0]).unwrap();
          for arg in cmd_args {
            exec_argv.push(CString::new(*arg).unwrap());
          }
        }

        let _ = execvp(&prog, &exec_argv);
        eprintln!("flock: can't execute '{}': {}", prog.to_string_lossy(), std::io::Error::last_os_error());
        std::process::exit(127);
      }
      Err(e) => {
        eprintln!("flock: fork failed: {}", e);
        return 1;
      }
    }
  }

  0
}
