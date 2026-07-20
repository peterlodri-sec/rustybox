//! `gzip` / `gunzip` / `zcat` backed by `flate2` (miniz_oxide, pure Rust).
//!
//! Common surface: compress/decompress files or stdin↔stdout, `-c` (stdout),
//! `-d` (decompress), `-k` (keep input), `-f` (force overwrite), `-1..-9`
//! (level). Applet name sets the default mode: `gunzip`/`zcat` decompress,
//! `zcat` implies `-c`.

use std::fs::File;
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::Path;

use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;

pub fn run(name: &str, argv: &[&str]) -> i32 {
  let prog = name.rsplit('/').next().unwrap_or(name);
  let mut decompress = prog == "gunzip" || prog == "zcat";
  let mut to_stdout = prog == "zcat";
  let mut keep = false;
  let mut force = false;
  let mut level = 6u32;
  let mut files: Vec<String> = Vec::new();

  for a in argv.iter().skip(1).copied() {
    if a.len() > 1 && a.starts_with('-') && a != "-" {
      if let Some(long) = a.strip_prefix("--") {
        match long {
          "decompress" | "uncompress" => decompress = true,
          "stdout" | "to-stdout" => to_stdout = true,
          "keep" => keep = true,
          "force" => force = true,
          "version" => {
            println!("gzip (rustybox, flate2 engine)");
            return 0;
          }
          _ => {}
        }
        continue;
      }
      for c in a[1..].chars() {
        match c {
          'd' => decompress = true,
          'c' => to_stdout = true,
          'k' => keep = true,
          'f' => force = true,
          '1'..='9' => level = c.to_digit(10).unwrap(),
          'n' | 'q' | 'v' | 'N' => {} // accepted, no-op
          _ => {}
        }
      }
      continue;
    }
    files.push(a.to_string());
  }

  // stdin -> stdout
  if files.is_empty() {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let res = if decompress {
      decode(stdin.lock(), stdout.lock())
    } else {
      encode(stdin.lock(), stdout.lock(), level)
    };
    return report(prog, "(stdin)", res);
  }

  let mut rc = 0;
  for f in &files {
    let res = process(prog, f, decompress, to_stdout, keep, force, level);
    if report(prog, f, res) != 0 {
      rc = 1;
    }
  }
  rc
}

fn process(
  _prog: &str,
  f: &str,
  decompress: bool,
  to_stdout: bool,
  keep: bool,
  force: bool,
  level: u32,
) -> io::Result<()> {
  if decompress {
    let out = f
      .strip_suffix(".gz")
      .or_else(|| f.strip_suffix(".tgz").map(|_| f))
      .map(|s| if f.ends_with(".tgz") { format!("{s}.tar") } else { s.to_string() });
    let input = BufReader::new(File::open(f)?);
    if to_stdout {
      decode(input, io::stdout().lock())?;
    } else {
      let out = out.ok_or_else(|| err(&format!("{f}: unknown suffix -- ignored")))?;
      guard_exists(&out, force)?;
      decode(input, BufWriter::new(File::create(&out)?))?;
      if !keep {
        std::fs::remove_file(f)?;
      }
    }
  } else {
    let input = BufReader::new(File::open(f)?);
    if to_stdout {
      encode(input, io::stdout().lock(), level)?;
    } else {
      let out = format!("{f}.gz");
      guard_exists(&out, force)?;
      encode(input, BufWriter::new(File::create(&out)?), level)?;
      if !keep {
        std::fs::remove_file(f)?;
      }
    }
  }
  Ok(())
}

fn encode<R: Read, W: Write>(mut src: R, dst: W, level: u32) -> io::Result<()> {
  let mut enc = GzEncoder::new(dst, Compression::new(level));
  io::copy(&mut src, &mut enc)?;
  enc.finish()?;
  Ok(())
}

fn decode<R: Read, W: Write>(src: R, mut dst: W) -> io::Result<()> {
  let mut dec = GzDecoder::new(src);
  io::copy(&mut dec, &mut dst)?;
  Ok(())
}

fn guard_exists(path: &str, force: bool) -> io::Result<()> {
  if !force && Path::new(path).exists() {
    return Err(err(&format!("{path} already exists (use -f to overwrite)")));
  }
  Ok(())
}

fn err(msg: &str) -> io::Error {
  io::Error::new(io::ErrorKind::Other, msg.to_string())
}

fn report(prog: &str, what: &str, res: io::Result<()>) -> i32 {
  match res {
    Ok(()) => 0,
    Err(e) => {
      eprintln!("{prog}: {what}: {e}");
      1
    }
  }
}
