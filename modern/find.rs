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

enum Pred {
  Name(globset::GlobMatcher),
  Path(globset::GlobMatcher),
  Type(char),
  Empty,
  Not(Box<Pred>),
}

enum Action {
  Print,
  Print0,
  Delete,
  Exec { argv: Vec<String>, batch: bool },
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

  for start in &paths {
    let walker = WalkDir::new(start)
      .min_depth(min_depth)
      .max_depth(max_depth);
    for entry in walker {
      let entry = match entry {
        Ok(e) => e,
        Err(e) => {
          eprintln!("find: {e}");
          rc = 1;
          continue;
        }
      };
      if !preds.iter().all(|p| eval(p, &entry)) {
        continue;
      }
      let path = entry.path().to_string_lossy();
      for act in &actions {
        match act {
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

fn eval(p: &Pred, e: &walkdir::DirEntry) -> bool {
  match p {
    Pred::Not(inner) => !eval(inner, e),
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
