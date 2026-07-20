//! `grep`/`egrep`/`fgrep` backed by ripgrep's library crates
//! (`grep-regex` + `grep-searcher`), with recursion via `ignore`.
//!
//! Covers the common BusyBox/POSIX grep surface: `-i -v -n -c -l -q -F -w -r/-R
//! -h -H -s -e`, patterns from args or `-e`, files or stdin. Exit status
//! follows grep convention: 0 = match found, 1 = no match, 2 = error.

use std::io::{self, Write};
use std::path::Path;

use grep_regex::RegexMatcherBuilder;
use grep_searcher::{Searcher, SearcherBuilder, Sink, SinkMatch};

#[derive(Default)]
struct Opts {
  ignore_case: bool,
  invert: bool,
  count: bool,
  line_number: bool,
  files_with_matches: bool,
  quiet: bool,
  fixed: bool,
  word: bool,
  recursive: bool,
  no_filename: bool,
  with_filename: bool,
  no_messages: bool,
  patterns: Vec<String>,
  files: Vec<String>,
}

pub fn run(name: &str, argv: &[&str]) -> i32 {
  let prog = name.rsplit('/').next().unwrap_or(name);
  let mut o = Opts::default();
  o.fixed = prog == "fgrep";

  let mut args = argv.iter().skip(1).copied();
  let mut pattern_from_flag = false;
  let mut only_files = false; // after "--"
  while let Some(arg) = args.next() {
    if !only_files && arg == "--" {
      only_files = true;
      continue;
    }
    if !only_files && arg.starts_with('-') && arg.len() > 1 {
      if let Some(rest) = arg.strip_prefix("--") {
        match rest {
          "ignore-case" => o.ignore_case = true,
          "invert-match" => o.invert = true,
          "count" => o.count = true,
          "line-number" => o.line_number = true,
          "files-with-matches" => o.files_with_matches = true,
          "quiet" | "silent" => o.quiet = true,
          "fixed-strings" => o.fixed = true,
          "word-regexp" => o.word = true,
          "recursive" => o.recursive = true,
          "no-filename" => o.no_filename = true,
          "with-filename" => o.with_filename = true,
          "no-messages" => o.no_messages = true,
          "regexp" => {
            if let Some(p) = args.next() {
              o.patterns.push((*p).to_string());
              pattern_from_flag = true;
            }
          }
          _ => {}
        }
        continue;
      }
      // bundled short flags, e.g. -in, -e PATTERN
      let mut chars = arg[1..].chars().peekable();
      while let Some(c) = chars.next() {
        match c {
          'i' => o.ignore_case = true,
          'v' => o.invert = true,
          'c' => o.count = true,
          'n' => o.line_number = true,
          'l' => o.files_with_matches = true,
          'q' => o.quiet = true,
          'F' => o.fixed = true,
          'w' => o.word = true,
          'r' | 'R' => o.recursive = true,
          'h' => o.no_filename = true,
          'H' => o.with_filename = true,
          's' => o.no_messages = true,
          'e' => {
            let val: String = chars.collect();
            let p = if val.is_empty() {
              args.next().map(|s| (*s).to_string())
            } else {
              Some(val)
            };
            if let Some(p) = p {
              o.patterns.push(p);
              pattern_from_flag = true;
            }
            break;
          }
          _ => {}
        }
      }
      continue;
    }
    // positional: first is the pattern (unless -e was used), rest are files
    if o.patterns.is_empty() && !pattern_from_flag {
      o.patterns.push(arg.to_string());
    } else {
      o.files.push(arg.to_string());
    }
  }

  if o.patterns.is_empty() {
    eprintln!("{}: no pattern", prog);
    return 2;
  }

  let mut builder = RegexMatcherBuilder::new();
  builder.case_insensitive(o.ignore_case).word(o.word);
  let pats: Vec<String> = o
    .patterns
    .iter()
    .map(|p| if o.fixed { regex_escape(p) } else { p.clone() })
    .collect();
  let matcher = match builder.build_many(&pats) {
    Ok(m) => m,
    Err(e) => {
      if !o.no_messages {
        eprintln!("{}: {}", prog, e);
      }
      return 2;
    }
  };

  let mut searcher = SearcherBuilder::new()
    .line_number(o.line_number)
    .invert_match(o.invert)
    .build();

  // Collect the concrete file list (expanding directories when recursive).
  let mut sources: Vec<Option<String>> = Vec::new(); // None = stdin
  if o.files.is_empty() {
    if o.recursive {
      sources.push(Some(".".to_string()));
    } else {
      sources.push(None);
    }
  } else {
    for f in &o.files {
      sources.push(Some(f.clone()));
    }
  }

  // Filename prefixing rules, POSIX-ish.
  let mut expanded: Vec<Option<String>> = Vec::new();
  for s in sources {
    match s {
      None => expanded.push(None),
      Some(p) => {
        if o.recursive && Path::new(&p).is_dir() {
          for entry in ignore::WalkBuilder::new(&p).standard_filters(false).build() {
            if let Ok(e) = entry {
              if e.file_type().map(|t| t.is_file()).unwrap_or(false) {
                expanded.push(Some(e.path().to_string_lossy().into_owned()));
              }
            }
          }
        } else {
          expanded.push(Some(p));
        }
      }
    }
  }

  let show_filename = !o.no_filename
    && (o.with_filename || o.recursive || expanded.iter().filter(|s| s.is_some()).count() > 1);

  let mut any_match = false;
  let mut had_error = false;
  let stdout = io::stdout();
  let mut out = stdout.lock();

  for src in &expanded {
    let path_label = match src {
      Some(p) if show_filename => Some(p.clone()),
      _ => None,
    };
    let mut sink = GrepSink {
      wtr: &mut out,
      path: path_label,
      line_number: o.line_number,
      count_only: o.count,
      files_with_matches: o.files_with_matches,
      quiet: o.quiet,
      count: 0,
      matched: false,
    };
    let res = match src {
      None => searcher.search_reader(&matcher, io::stdin().lock(), &mut sink),
      Some(p) => searcher.search_path(&matcher, Path::new(p), &mut sink),
    };
    let matched = sink.matched;
    let count = sink.count;
    if let Err(e) = res {
      if !o.no_messages {
        let where_ = src.as_deref().unwrap_or("(standard input)");
        eprintln!("{}: {}: {}", prog, where_, e);
      }
      had_error = true;
      continue;
    }
    if matched {
      any_match = true;
    }
    if o.quiet && any_match {
      return 0;
    }
    if o.count {
      if show_filename {
        if let Some(p) = src {
          let _ = write!(out, "{}:", p);
        }
      }
      let _ = writeln!(out, "{}", count);
    } else if o.files_with_matches && matched {
      if let Some(p) = src {
        let _ = writeln!(out, "{}", p);
      }
    }
  }

  if had_error {
    2
  } else if any_match {
    0
  } else {
    1
  }
}

struct GrepSink<'a, W: Write> {
  wtr: &'a mut W,
  path: Option<String>,
  line_number: bool,
  count_only: bool,
  files_with_matches: bool,
  quiet: bool,
  count: u64,
  matched: bool,
}

impl<'a, W: Write> Sink for GrepSink<'a, W> {
  type Error = io::Error;
  fn matched(&mut self, _searcher: &Searcher, mat: &SinkMatch) -> Result<bool, io::Error> {
    self.matched = true;
    self.count += 1;
    if self.quiet || self.files_with_matches {
      return Ok(false); // seen a match; nothing more to do for this source
    }
    if self.count_only {
      return Ok(true); // keep counting, don't print lines
    }
    if let Some(p) = &self.path {
      write!(self.wtr, "{}:", p)?;
    }
    if self.line_number {
      if let Some(n) = mat.line_number() {
        write!(self.wtr, "{}:", n)?;
      }
    }
    self.wtr.write_all(mat.bytes())?;
    Ok(true)
  }
}

/// Escape regex metacharacters for `-F`/`fgrep` fixed-string matching.
fn regex_escape(s: &str) -> String {
  let mut out = String::with_capacity(s.len());
  for c in s.chars() {
    if "\\.^$|?*+()[]{}".contains(c) {
      out.push('\\');
    }
    out.push(c);
  }
  out
}
