use std::io::Write;

pub fn yes_main(args: &[&str]) -> ! {
  #[cfg(unix)]
  unsafe {
    libc::signal(libc::SIGPIPE, libc::SIG_IGN);
  }

  let line = if args.len() > 1 {
    args[1..].join(" ")
  } else {
    "y".to_string()
  };

  let stdout = std::io::stdout();
  let mut handle = stdout.lock();
  while writeln!(handle, "{}", line).is_ok() {}
  std::process::exit(0);
}
