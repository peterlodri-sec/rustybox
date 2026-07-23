//! `find` backed by `walkdir` (traversal) + `globset` (glob matching).
//!
//! Common BusyBox/POSIX surface: starting paths; predicates `-name -iname
//! -path -ipath -type -maxdepth -mindepth -empty -not/!`; actions `-print
//! -print0 -delete -exec ... {} ;` and `-exec ... {} +`. Predicates are ANDed
//! (implicit `-a`); `-o` / grouping are not supported (rare in practice).

use std::io::{self, Write};
use std::process::Command;

use globset::GlobBuilder;
use walkdir::WalkDir;
use std::os::unix::fs::MetadataExt;
use std::time::SystemTime;

fn parse_num_cmp(s: &str) -> Option<(i64, char)> {
    if let Some(rest) = s.strip_prefix('+') {
        rest.parse().ok().map(|n| (n, '+'))
    } else if let Some(rest) = s.strip_prefix('-') {
        rest.parse().ok().map(|n| (n, '-'))
    } else {
        s.parse().ok().map(|n| (n, '='))
    }
}

fn parse_size(s: &str) -> Option<(u64, char)> {
    let cmp = if s.starts_with('+') {
        '+'
    } else if s.starts_with('-') {
        '-'
    } else {
        '='
    };
    let s = s.trim_start_matches(|c| c == '+' || c == '-');

    let mut num_str = String::new();
    let mut suffix = 'c'; // default for some is 'b' but find uses 'b' for 512, 'c' for 1
    for c in s.chars() {
        if c.is_ascii_digit() {
            num_str.push(c);
        } else {
            suffix = c;
            break;
        }
    }
    if num_str.is_empty() {
        // default 512-byte blocks in POSIX find if no suffix, wait let's just parse the digits.
        return None;
    }
    let n: u64 = num_str.parse().ok()?;
    let mult = match suffix {
        'c' => 1,
        'b' | 'w' => 512, // w is 2, b is 512. Wait, if suffix wasn't provided, standard says 512-byte blocks!
        'k' => 1024,
        'M' => 1024 * 1024,
        'G' => 1024 * 1024 * 1024,
        _ => return None,
    };
    // Busybox standard default without suffix is 512 byte blocks.
    let mult = if s.chars().all(|c| c.is_ascii_digit()) {
        512
    } else {
        mult
    };
    Some((n * mult, cmp))
}

fn parse_uid(s: &str) -> Option<u32> {
    if let Ok(uid) = s.parse::<u32>() {
        return Some(uid);
    }
    use std::ffi::CString;
    let cstr = CString::new(s).ok()?;
    unsafe {
        let pw = libc::getpwnam(cstr.as_ptr());
        if !pw.is_null() {
            return Some((*pw).pw_uid);
        }
    }
    None
}

fn parse_gid(s: &str) -> Option<u32> {
    if let Ok(gid) = s.parse::<u32>() {
        return Some(gid);
    }
    use std::ffi::CString;
    let cstr = CString::new(s).ok()?;
    unsafe {
        let gr = libc::getgrnam(cstr.as_ptr());
        if !gr.is_null() {
            return Some((*gr).gr_gid);
        }
    }
    None
}

fn parse_perm(s: &str) -> Option<(u32, char)> {
    let cmp = if s.starts_with('-') {
        '-'
    } else if s.starts_with('/') {
        '/'
    } else if s.starts_with('+') {
        '+' 
    } else {
        '='
    };
    let s = s.trim_start_matches(|c| c == '-' || c == '/' || c == '+');
    u32::from_str_radix(s, 8).ok().map(|n| (n, cmp))
}

enum Pred {
  Name(globset::GlobMatcher),
  Path(globset::GlobMatcher),
  Type(char),
  Empty,
  Not(Box<Pred>),
  Mtime(i64, char),
  Mmin(i64, char),
  Newer(SystemTime),
  Size(u64, char),
  Inum(u64),
  Links(u64, char),
  Perm(u32, char),
  Executable,
  User(u32),
  Group(u32),
  Prune,
}

enum Action {
  Print,
  Print0,
  Delete,
  Exec { argv: Vec<String>, batch: bool },
  Quit,
}

pub fn run_and_exit(argv: &[&str]) -> ! {
  let rc = run(argv);
  std::process::exit(rc);
}

pub fn run(argv: &[&str]) -> i32 {
  let mut args = argv.iter().skip(1).copied().peekable();

  // Leading operands are starting paths (until the first token that looks like
  // an expression: starts with '-' or is '!' '(' ')').
  let mut paths: Vec<String> = Vec::new();
  while let Some(&t) = args.peek() {
    if t.starts_with('-') || t == "!" || t == "(" || t == ")" {
      break;
    }
    paths.push(t.to_string());
    args.next();
  }
  if paths.is_empty() {
    paths.push(".".to_string());
  }

  let mut preds: Vec<Pred> = Vec::new();
  let mut actions: Vec<Action> = Vec::new();
  let mut max_depth = usize::MAX;
  let mut min_depth = 0usize;
  let mut negate_next = false;

  let glob = |pat: &str, icase: bool| -> Result<globset::GlobMatcher, globset::Error> {
    Ok(
      GlobBuilder::new(pat)
        .case_insensitive(icase)
        .literal_separator(false)
        .build()?
        .compile_matcher(),
    )
  };

  macro_rules! push_pred {
    ($p:expr) => {{
      let p = $p;
      preds.push(if negate_next {
        Pred::Not(Box::new(p))
      } else {
        p
      });
      negate_next = false;
    }};
  }

  while let Some(tok) = args.next() {
    match tok {
      "!" | "-not" => negate_next = !negate_next,
      "-name" | "-iname" => match args.next() {
        Some(p) => match glob(p, tok == "-iname") {
          Ok(m) => push_pred!(Pred::Name(m)),
          Err(e) => return err(&format!("bad glob {p}: {e}")),
        },
        None => return err("-name needs an argument"),
      },
      "-path" | "-ipath" | "-wholename" => match args.next() {
        Some(p) => match glob(p, tok == "-ipath") {
          Ok(m) => push_pred!(Pred::Path(m)),
          Err(e) => return err(&format!("bad glob {p}: {e}")),
        },
        None => return err("-path needs an argument"),
      },
      "-type" => match args.next() {
        Some(t) if t.len() == 1 => push_pred!(Pred::Type(t.chars().next().unwrap())),
        _ => return err("-type needs f/d/l"),
      },
      "-empty" => push_pred!(Pred::Empty),
      "-maxdepth" => match args.next().and_then(|s| s.parse().ok()) {
        Some(n) => max_depth = n,
        None => return err("-maxdepth needs a number"),
      },
      "-mindepth" => match args.next().and_then(|s| s.parse().ok()) {
        Some(n) => min_depth = n,
        None => return err("-mindepth needs a number"),
      },
      "-mtime" => match args.next().and_then(|s| parse_num_cmp(s)) {
        Some((n, c)) => push_pred!(Pred::Mtime(n, c)),
        None => return err("-mtime needs a number"),
      },
      "-mmin" => match args.next().and_then(|s| parse_num_cmp(s)) {
        Some((n, c)) => push_pred!(Pred::Mmin(n, c)),
        None => return err("-mmin needs a number"),
      },
      "-newer" => match args.next() {
        Some(f) => {
            if let Ok(meta) = std::fs::metadata(f) {
                if let Ok(mtime) = meta.modified() {
                    push_pred!(Pred::Newer(mtime))
                } else {
                    return err(&format!("{f} mtime read failed"));
                }
            } else {
                return err(&format!("{f} not found"));
            }
        },
        None => return err("-newer needs a file"),
      },
      "-size" => match args.next().and_then(|s| parse_size(s)) {
        Some((n, c)) => push_pred!(Pred::Size(n, c)),
        None => return err("-size needs a number"),
      },
      "-inum" => match args.next().and_then(|s| s.parse().ok()) {
        Some(n) => push_pred!(Pred::Inum(n)),
        None => return err("-inum needs a number"),
      },
      "-links" => match args.next().and_then(|s| parse_num_cmp(s)) {
        Some((n, c)) => push_pred!(Pred::Links(n as u64, c)),
        None => return err("-links needs a number"),
      },
      "-perm" => match args.next().and_then(|s| parse_perm(s)) {
        Some((n, c)) => push_pred!(Pred::Perm(n, c)),
        None => return err("-perm needs an octal mode"),
      },
      "-executable" => push_pred!(Pred::Executable),
      "-user" => match args.next().and_then(|s| parse_uid(s)) {
        Some(u) => push_pred!(Pred::User(u)),
        None => return err("-user needs a valid username or uid"),
      },
      "-group" => match args.next().and_then(|s| parse_gid(s)) {
        Some(g) => push_pred!(Pred::Group(g)),
        None => return err("-group needs a valid groupname or gid"),
      },
      "-prune" => push_pred!(Pred::Prune),
      "-quit" => actions.push(Action::Quit),
      "-print" => actions.push(Action::Print),
      "-print0" => actions.push(Action::Print0),
      "-delete" => actions.push(Action::Delete),
      "-exec" => {
        let mut cmd = Vec::new();
        let mut batch = false;
        loop {
          match args.next() {
            Some(";") => break,
            Some("+") => {
              batch = true;
              break;
            }
            Some(a) => cmd.push(a.to_string()),
            None => return err("-exec not terminated by ; or +"),
          }
        }
        actions.push(Action::Exec { argv: cmd, batch });
      }
      other => return err(&format!("unsupported predicate: {other}")),
    }
  }

  if actions.is_empty() {
    actions.push(Action::Print);
  }

  let stdout = io::stdout();
  let mut out = stdout.lock();
  let mut rc = 0;
  let mut batch_paths: Vec<String> = Vec::new();

  'outer: for start in &paths {
    let mut walker = WalkDir::new(start)
      .min_depth(min_depth)
      .max_depth(max_depth)
      .into_iter();
    while let Some(entry) = walker.next() {
      let entry = match entry {
        Ok(e) => e,
        Err(e) => {
          eprintln!("find: {e}");
          rc = 1;
          continue;
        }
      };
      if !preds.iter().all(|p| eval(p, &entry, &mut walker)) {
        continue;
      }
      let path = entry.path().to_string_lossy();
      for act in &actions {
        match act {
          Action::Quit => break 'outer,
          Action::Print => {
            let _ = writeln!(out, "{path}");
          }
          Action::Print0 => {
            let _ = out.write_all(path.as_bytes());
            let _ = out.write_all(&[0]);
          }
          Action::Delete => {
            let r = if entry.file_type().is_dir() {
              std::fs::remove_dir(entry.path())
            } else {
              std::fs::remove_file(entry.path())
            };
            if let Err(e) = r {
              eprintln!("find: cannot delete '{path}': {e}");
              rc = 1;
            }
          }
          Action::Exec { argv, batch } => {
            if *batch {
              batch_paths.push(path.to_string());
            } else if !run_exec(argv, &[path.to_string()]) {
              rc = 1;
            }
          }
        }
      }
    }
  }

  // Deferred `-exec ... +` batch.
  if !batch_paths.is_empty() {
    for act in &actions {
      if let Action::Exec { argv, batch: true } = act {
        if !run_exec(argv, &batch_paths) {
          rc = 1;
        }
      }
    }
  }

  rc
}

fn eval(p: &Pred, e: &walkdir::DirEntry, walker: &mut walkdir::IntoIter) -> bool {
  match p {
    Pred::Not(inner) => !eval(inner, e, walker),
    Pred::Name(m) => e
      .file_name()
      .to_str()
      .map(|n| m.is_match(n))
      .unwrap_or(false),
    Pred::Path(m) => m.is_match(e.path()),
    Pred::Type(t) => {
      let ft = e.file_type();
      match t {
        'f' => ft.is_file(),
        'd' => ft.is_dir(),
        'l' => ft.is_symlink(),
        _ => false,
      }
    }
    Pred::Empty => {
      let ft = e.file_type();
      if ft.is_file() {
        e.metadata().map(|m| m.len() == 0).unwrap_or(false)
      } else if ft.is_dir() {
        std::fs::read_dir(e.path())
          .map(|mut d| d.next().is_none())
          .unwrap_or(false)
      } else {
        false
      }
    }
    Pred::Prune => {
      walker.skip_current_dir();
      true
    }
    Pred::Mtime(n, c) => {
      if let Ok(meta) = e.metadata() {
        if let Ok(mtime) = meta.modified() {
          if let Ok(elapsed) = mtime.elapsed() {
            let days = (elapsed.as_secs() / 86400) as i64;
            match c {
              '+' => days > *n,
              '-' => days < *n,
              _ => days == *n,
            }
          } else { false }
        } else { false }
      } else { false }
    }
    Pred::Mmin(n, c) => {
      if let Ok(meta) = e.metadata() {
        if let Ok(mtime) = meta.modified() {
          if let Ok(elapsed) = mtime.elapsed() {
            let mins = (elapsed.as_secs() / 60) as i64;
            match c {
              '+' => mins > *n,
              '-' => mins < *n,
              _ => mins == *n,
            }
          } else { false }
        } else { false }
      } else { false }
    }
    Pred::Newer(time) => {
      e.metadata().ok().and_then(|m| m.modified().ok()).map(|m| m > *time).unwrap_or(false)
    }
    Pred::Size(n, c) => {
      e.metadata().map(|m| {
        let size = m.len();
        match c {
          '+' => size > *n,
          '-' => size < *n,
          _ => size == *n,
        }
      }).unwrap_or(false)
    }
    Pred::Inum(n) => {
      e.metadata().map(|m| m.ino() == *n).unwrap_or(false)
    }
    Pred::Links(n, c) => {
      e.metadata().map(|m| {
        let links = m.nlink();
        match c {
          '+' => links > *n,
          '-' => links < *n,
          _ => links == *n,
        }
      }).unwrap_or(false)
    }
    Pred::Perm(n, c) => {
      e.metadata().map(|m| {
        let mode = m.mode() & 0o7777;
        match c {
          '-' => (mode & *n) == *n,
          '/' | '+' => (mode & *n) != 0 || *n == 0,
          _ => mode == *n,
        }
      }).unwrap_or(false)
    }
    Pred::Executable => {
      use std::ffi::CString;
      if let Some(path) = e.path().to_str() {
        if let Ok(cpath) = CString::new(path) {
          unsafe { libc::access(cpath.as_ptr(), libc::X_OK) == 0 }
        } else { false }
      } else { false }
    }
    Pred::User(u) => {
      e.metadata().map(|m| m.uid() == *u).unwrap_or(false)
    }
    Pred::Group(g) => {
      e.metadata().map(|m| m.gid() == *g).unwrap_or(false)
    }
  }
}

/// Run `-exec` argv, substituting `{}` with each path (all paths for `+`).
fn run_exec(template: &[String], paths: &[String]) -> bool {
  if template.is_empty() {
    return false;
  }
  let mut built: Vec<String> = Vec::new();
  let mut saw_brace = false;
  for tok in template {
    if tok == "{}" {
      saw_brace = true;
      built.extend(paths.iter().cloned());
    } else {
      built.push(tok.clone());
    }
  }
  if !saw_brace {
    built.extend(paths.iter().cloned());
  }
  let (cmd, rest) = built.split_first().unwrap();
  match Command::new(cmd).args(rest).status() {
    Ok(s) => s.success(),
    Err(e) => {
      eprintln!("find: {cmd}: {e}");
      false
    }
  }
}

fn err(msg: &str) -> i32 {
  eprintln!("find: {msg}");
  2
}
