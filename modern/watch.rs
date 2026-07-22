use std::process::Command;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn get_terminal_width() -> usize {
  // Basic fallback width; you can improve this with libc::ioctl and TIOCGWINSZ if needed.
  // In busybox it was xfuncs::get_terminal_width(2).
  let mut winsize = libc::winsize {
    ws_row: 0,
    ws_col: 0,
    ws_xpixel: 0,
    ws_ypixel: 0,
  };
  unsafe {
    if libc::ioctl(libc::STDERR_FILENO, libc::TIOCGWINSZ, &mut winsize) == 0 && winsize.ws_col > 0 {
      return winsize.ws_col as usize;
    }
  }
  80
}

fn format_time(time: SystemTime) -> String {
  // Very simple time formatting for the watch header, e.g., YYYY-MM-DD HH:MM:SS
  let duration = time.duration_since(UNIX_EPOCH).unwrap_or_default();
  let secs = duration.as_secs() as i64;
  unsafe {
    let mut tm = std::mem::zeroed();
    libc::localtime_r(&secs as *const _ as *const libc::time_t, &mut tm);
    format!(
      "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
      tm.tm_year + 1900,
      tm.tm_mon + 1,
      tm.tm_mday,
      tm.tm_hour,
      tm.tm_min,
      tm.tm_sec
    )
  }
}

pub fn run(argv: &[&str]) -> i32 {
  let mut args = argv.iter().skip(1).peekable();
  let mut period = 2.0;
  let mut no_title = false;

  // A very simple arg parser for POSIX-ish watch
  while let Some(&arg) = args.peek() {
    if *arg == "-t" || *arg == "--no-title" {
      no_title = true;
      args.next();
    } else if *arg == "-n" || *arg == "--interval" {
      args.next();
      if let Some(val) = args.next() {
        if let Ok(p) = val.parse::<f64>() {
          period = p;
        }
      }
    } else if arg.starts_with("-n") {
      if let Ok(p) = arg[2..].parse::<f64>() {
        period = p;
      }
      args.next();
    } else if *arg == "-d" || *arg == "--differences" {
      // Ignored, we don't support highlighting differences in this minimal version
      args.next();
    } else if *arg == "--" {
      args.next();
      break;
    } else if arg.starts_with("-") {
      // Unknown or unsupported option, break to treat as command
      break;
    } else {
      // First non-option
      break;
    }
  }

  if period < 0.1 {
    period = 0.1;
  }

  let command_args: Vec<&str> = args.copied().collect();
  if command_args.is_empty() {
    eprintln!("watch: usage: watch [-n SEC] [-t] PROG ARGS");
    return 1;
  }

  // "watch" executes the command through `sh -c` and concatenates the arguments
  let cmd_str = command_args.join(" ");

  loop {
    // Clear screen and home cursor
    print!("\x1B[H\x1B[J");

    if !no_title {
      let width = get_terminal_width();
      let mut header = format!("Every {:.1}s: {}", period, cmd_str);

      let time_str = format_time(SystemTime::now());
      if header.len() + time_str.len() < width {
        let padding = width - header.len() - time_str.len();
        header.push_str(&" ".repeat(padding));
        header.push_str(&time_str);
      }

      println!("{}\n", header);
    }

    // flush stdout before executing the command so the header is visible
    use std::io::Write;
    let _ = std::io::stdout().flush();

    let mut child = Command::new("sh")
      .arg("-c")
      .arg(&cmd_str)
      .spawn()
      .expect("failed to execute shell");

    let _ = child.wait();

    thread::sleep(Duration::from_secs_f64(period));
  }
}

#[allow(dead_code)]
pub fn run_and_exit(argv: &[&str]) -> ! {
  std::process::exit(run(argv))
}
