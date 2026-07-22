use std::io::{self, Read};
use std::process::Command;

pub fn run(argv: &[&str]) -> i32 {
  let mut args = argv.iter().skip(1).copied().peekable();
  let mut null_sep = false;
  let mut verbose = false;
  let mut max_args = usize::MAX;

  while let Some(&arg) = args.peek() {
    if arg == "-0" || arg == "--null" {
      null_sep = true;
      args.next();
    } else if arg == "-t" || arg == "--verbose" {
      verbose = true;
      args.next();
    } else if arg == "-n" || arg == "--max-args" {
      args.next();
      if let Some(val) = args.next() {
        if let Ok(n) = val.parse::<usize>() {
          max_args = n;
        }
      }
    } else if arg.starts_with("-n") {
      if let Ok(n) = arg[2..].parse::<usize>() {
        max_args = n;
      }
      args.next();
    } else if arg == "--" {
      args.next();
      break;
    } else if arg.starts_with("-") {
      // unknown flag, break and treat as cmd? Usually xargs will parse options until first non-option
      args.next();
    } else {
      break;
    }
  }

  if max_args == 0 {
    max_args = usize::MAX;
  }

  let mut cmd_args: Vec<String> = args.map(|s| s.to_string()).collect();
  if cmd_args.is_empty() {
    cmd_args.push("echo".to_string());
  }

  let mut input = Vec::new();
  if io::stdin().read_to_end(&mut input).is_err() {
    return 1;
  }

  let mut tokens = Vec::new();
  if null_sep {
    for s in input.split(|&b| b == 0) {
      if !s.is_empty() {
        tokens.push(String::from_utf8_lossy(s).into_owned());
      }
    }
  } else {
    let input_str = String::from_utf8_lossy(&input);
    for s in input_str.split_whitespace() {
      tokens.push(s.to_string());
    }
  }

  if tokens.is_empty() {
    return 0; // nothing to do
  }

  let mut exit_code = 0;

  for chunk in tokens.chunks(max_args) {
    let mut final_args = cmd_args.clone();
    final_args.extend(chunk.iter().cloned());

    if verbose {
      eprintln!("{}", final_args.join(" "));
    }

    let (cmd, rest) = final_args.split_first().unwrap();
    match Command::new(cmd).args(rest).status() {
      Ok(s) => {
        if !s.success() {
          exit_code = 1;
        }
      }
      Err(e) => {
        eprintln!("xargs: {}: {}", cmd, e);
        return 127;
      }
    }
  }

  exit_code
}

#[allow(dead_code)]
pub fn run_and_exit(argv: &[&str]) -> ! {
  std::process::exit(run(argv));
}
