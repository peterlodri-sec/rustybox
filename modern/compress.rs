//! Generic (de)compressor for the gzip / bzip2 / xz applet families, sharing
//! one CLI implementation across codecs.
//!
//! Each family is a [`Codec`] (its applet names, suffix, encode/decode fns).
//! Common surface: compress/decompress files or stdin↔stdout, `-c` (stdout),
//! `-d` (decompress), `-k` (keep input), `-f` (force), `-1..-9` (level). Applet
//! name sets the default mode: `un*`/`*cat` decompress, `*cat` implies `-c`.

use std::fs::File;
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::Path;

pub struct Codec {
  pub comp: &'static str,   // e.g. "gzip"
  pub decomp: &'static str, // e.g. "gunzip"
  pub cat: &'static str,    // e.g. "zcat"
  pub suffix: &'static str, // e.g. ".gz"
  pub encode: fn(&mut dyn Read, &mut dyn Write, u32) -> io::Result<()>,
  pub decode: fn(&mut dyn Read, &mut dyn Write) -> io::Result<()>,
}

pub fn run(name: &str, argv: &[&str], c: &Codec) -> i32 {
  let prog = name.rsplit('/').next().unwrap_or(name);
  let mut decompress = prog == c.decomp || prog == c.cat;
  let mut to_stdout = prog == c.cat;
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
            println!("{} (rustybox)", c.comp);
            return 0;
          }
          _ => {}
        }
        continue;
      }
      for ch in a[1..].chars() {
        match ch {
          'd' => decompress = true,
          'c' => to_stdout = true,
          'k' => keep = true,
          'f' => force = true,
          '1'..='9' => level = ch.to_digit(10).unwrap(),
          _ => {} // -n -q -v -N -T... accepted, no-op
        }
      }
      continue;
    }
    files.push(a.to_string());
  }

  if files.is_empty() {
    let (r, w) = (io::stdin(), io::stdout());
    let mut rd = r.lock();
    let mut wr = w.lock();
    let res = if decompress {
      (c.decode)(&mut rd, &mut wr)
    } else {
      (c.encode)(&mut rd, &mut wr, level)
    };
    return report(c.comp, "(stdin)", res);
  }

  let mut rc = 0;
  for f in &files {
    if report(
      c.comp,
      f,
      process(f, decompress, to_stdout, keep, force, level, c),
    ) != 0
    {
      rc = 1;
    }
  }
  rc
}

fn process(
  f: &str,
  decompress: bool,
  to_stdout: bool,
  keep: bool,
  force: bool,
  level: u32,
  c: &Codec,
) -> io::Result<()> {
  let mut input = BufReader::new(File::open(f)?);
  if decompress {
    if to_stdout {
      (c.decode)(&mut input, &mut io::stdout().lock())?;
    } else {
      let out = f
        .strip_suffix(c.suffix)
        .ok_or_else(|| err(&format!("{f}: unknown suffix -- ignored")))?
        .to_string();
      guard(&out, force)?;
      let mut w = BufWriter::new(File::create(&out)?);
      (c.decode)(&mut input, &mut w)?;
      w.flush()?;
      if !keep {
        std::fs::remove_file(f)?;
      }
    }
  } else if to_stdout {
    (c.encode)(&mut input, &mut io::stdout().lock(), level)?;
  } else {
    let out = format!("{f}{}", c.suffix);
    guard(&out, force)?;
    let mut w = BufWriter::new(File::create(&out)?);
    (c.encode)(&mut input, &mut w, level)?;
    w.flush()?;
    if !keep {
      std::fs::remove_file(f)?;
    }
  }
  Ok(())
}

fn guard(path: &str, force: bool) -> io::Result<()> {
  if !force && Path::new(path).exists() {
    return Err(err(&format!("{path} already exists (use -f to overwrite)")));
  }
  Ok(())
}

fn err(m: &str) -> io::Error {
  io::Error::new(io::ErrorKind::Other, m.to_string())
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

// ---- codecs -------------------------------------------------------------

#[cfg(feature = "modern-gzip")]
pub const GZIP: Codec = Codec {
  comp: "gzip",
  decomp: "gunzip",
  cat: "zcat",
  suffix: ".gz",
  encode: gz_enc,
  decode: gz_dec,
};
#[cfg(feature = "modern-gzip")]
fn gz_enc(src: &mut dyn Read, dst: &mut dyn Write, level: u32) -> io::Result<()> {
  let mut e = flate2::write::GzEncoder::new(dst, flate2::Compression::new(level));
  io::copy(src, &mut e)?;
  e.finish()?;
  Ok(())
}
#[cfg(feature = "modern-gzip")]
fn gz_dec(src: &mut dyn Read, dst: &mut dyn Write) -> io::Result<()> {
  io::copy(&mut flate2::read::GzDecoder::new(src), dst)?;
  Ok(())
}

#[cfg(feature = "modern-bzip2")]
pub const BZIP2: Codec = Codec {
  comp: "bzip2",
  decomp: "bunzip2",
  cat: "bzcat",
  suffix: ".bz2",
  encode: bz_enc,
  decode: bz_dec,
};
#[cfg(feature = "modern-bzip2")]
fn bz_enc(src: &mut dyn Read, dst: &mut dyn Write, level: u32) -> io::Result<()> {
  let lvl = bzip2::Compression::new(level.clamp(1, 9));
  let mut e = bzip2::write::BzEncoder::new(dst, lvl);
  io::copy(src, &mut e)?;
  e.finish()?;
  Ok(())
}
#[cfg(feature = "modern-bzip2")]
fn bz_dec(src: &mut dyn Read, dst: &mut dyn Write) -> io::Result<()> {
  io::copy(&mut bzip2::read::BzDecoder::new(src), dst)?;
  Ok(())
}

#[cfg(feature = "modern-xz")]
pub const XZ: Codec = Codec {
  comp: "xz",
  decomp: "unxz",
  cat: "xzcat",
  suffix: ".xz",
  encode: xz_enc,
  decode: xz_dec,
};
#[cfg(feature = "modern-xz")]
fn xz_enc(src: &mut dyn Read, dst: &mut dyn Write, level: u32) -> io::Result<()> {
  let mut e = xz2::write::XzEncoder::new(dst, level.clamp(0, 9));
  io::copy(src, &mut e)?;
  e.finish()?;
  Ok(())
}
#[cfg(feature = "modern-xz")]
fn xz_dec(src: &mut dyn Read, dst: &mut dyn Write) -> io::Result<()> {
  io::copy(&mut xz2::read::XzDecoder::new(src), dst)?;
  Ok(())
}
