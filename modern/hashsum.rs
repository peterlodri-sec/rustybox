//! `md5sum`/`sha1sum`/`sha256sum`/`sha512sum`/`sha3sum` on RustCrypto's
//! `md-5`/`sha1`/`sha2`/`sha3` crates, instead of the transpiled version's
//! own hand-written hash implementations (`libbb/hash_md5_sha.rs`).
//!
//! MIGRATION.md's plan was uutils' `uu_hashsum`, but it's been stuck at
//! 0.5 for 7+ months (checked crates.io directly) while the rest of the
//! uutils family we depend on is at 0.9 — genuinely blocked, not a "just
//! wait a bit" situation. RustCrypto's per-algorithm crates are a better
//! fit anyway: audited, widely used well beyond uutils, and each exposes
//! the exact same `Digest` trait (`new`/`update`/`finalize`), so all five
//! algorithms share one small dispatcher instead of needing a full
//! coreutils-wrapper crate's surface.
//!
//! Covers the full CLI surface: plain hashing of files or stdin (`-`),
//! `-c` check mode against a checksum file (accepting both the `HASH
//! filename` and `HASH *filename` binary-marker separators), `-s`
//! (status-only) and `-w` (warn on malformed lines), `-b`/`-t` (accepted,
//! no-ops — GNU compat, this implementation has no binary/text mode
//! distinction), and sha3sum's `-a WIDTH` output-width flag, restricted to
//! the four values that are actually standardized SHA3 (224/256/384/512
//! per FIPS 202) rather than the transpiled version's looser "any multiple
//! of 32" — anything else isn't a real SHA3 variant, just an artifact of
//! the hand-rolled sponge construction allowing arbitrary parameters.

use std::fs::File;
use std::io::{self, Read, Write};

use md5::Md5;
use sha1::Sha1;
use sha2::{Digest, Sha256, Sha512};
use sha3::{Sha3_224, Sha3_256, Sha3_384, Sha3_512};

enum Algo {
  Md5,
  Sha1,
  Sha256,
  Sha512,
  Sha3(u32),
}

fn hex_encode(bytes: &[u8]) -> String {
  bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn hash_reader(algo: &Algo, mut r: impl Read) -> io::Result<String> {
  let mut buf = [0u8; 65536];
  macro_rules! digest_loop {
    ($hasher:expr) => {{
      let mut h = $hasher;
      loop {
        let n = r.read(&mut buf)?;
        if n == 0 {
          break;
        }
        h.update(&buf[..n]);
      }
      hex_encode(&h.finalize())
    }};
  }
  Ok(match algo {
    Algo::Md5 => digest_loop!(Md5::new()),
    Algo::Sha1 => digest_loop!(Sha1::new()),
    Algo::Sha256 => digest_loop!(Sha256::new()),
    Algo::Sha512 => digest_loop!(Sha512::new()),
    Algo::Sha3(224) => digest_loop!(Sha3_224::new()),
    Algo::Sha3(256) => digest_loop!(Sha3_256::new()),
    Algo::Sha3(384) => digest_loop!(Sha3_384::new()),
    Algo::Sha3(512) => digest_loop!(Sha3_512::new()),
    Algo::Sha3(_) => unreachable!("validated in arg parsing"),
  })
}

fn hash_path(algo: &Algo, path: &str) -> io::Result<String> {
  if path == "-" {
    hash_reader(algo, io::stdin().lock())
  } else {
    hash_reader(algo, File::open(path)?)
  }
}

struct Opts {
  check: bool,
  status: bool,
  warn: bool,
  width: u32,
}

fn set_width(o: &mut Opts, v: &str) -> Result<(), String> {
  o.width = v.parse().map_err(|_| format!("sha3sum: bad -a{v}"))?;
  if !matches!(o.width, 224 | 256 | 384 | 512) {
    return Err(format!(
      "sha3sum: bad -a{}: must be 224, 256, 384, or 512",
      o.width
    ));
  }
  Ok(())
}

fn parse_args<'a>(algo_is_sha3: bool, argv: &[&'a str]) -> Result<(Opts, Vec<&'a str>), String> {
  let mut o = Opts {
    check: false,
    status: false,
    warn: false,
    width: 224,
  };
  let mut files = Vec::new();
  let mut it = argv.iter().skip(1).copied();
  while let Some(arg) = it.next() {
    match arg {
      "-c" | "--check" => o.check = true,
      "-s" | "--status" => o.status = true,
      "-w" | "--warn" => o.warn = true,
      "-b" | "--binary" | "-t" | "--text" => {} // GNU compat no-ops
      "-a" if algo_is_sha3 => {
        let v = it.next().ok_or("sha3sum: -a requires a width")?;
        set_width(&mut o, v)?;
      }
      _ if algo_is_sha3 && arg.starts_with("-a") && arg.len() > 2 => {
        set_width(&mut o, &arg[2..])?;
      }
      _ if arg.starts_with('-') && arg.len() > 1 => {
        return Err(format!("{arg}: unrecognized option"));
      }
      _ => files.push(arg),
    }
  }
  if files.is_empty() {
    files.push("-");
  }
  Ok((o, files))
}

fn print_hashes(algo: &Algo, files: &[&str]) -> i32 {
  let mut status = 0;
  for path in files {
    match hash_path(algo, path) {
      Ok(hex) => println!("{hex}  {path}"),
      Err(e) => {
        eprintln!("{path}: {e}");
        status = 1;
      }
    }
  }
  status
}

fn check_files(algo: &Algo, prog: &str, files: &[&str], o: &Opts) -> i32 {
  let mut status = 0;
  for list_path in files {
    let content = if *list_path == "-" {
      let mut s = String::new();
      if let Err(e) = io::stdin().read_to_string(&mut s) {
        eprintln!("{prog}: {list_path}: {e}");
        status = 1;
        continue;
      }
      s
    } else {
      match std::fs::read_to_string(list_path) {
        Ok(s) => s,
        Err(e) => {
          eprintln!("{prog}: {list_path}: {e}");
          status = 1;
          continue;
        }
      }
    };

    let mut total = 0;
    let mut failed = 0;
    for line in content.lines() {
      let split = line.find("  ").or_else(|| line.find(" *"));
      let Some(idx) = split else {
        if o.warn {
          eprintln!("{prog}: invalid format");
        }
        failed += 1;
        status = 1;
        continue;
      };
      total += 1;
      let expected = &line[..idx];
      let filename = &line[idx + 2..];
      let ok = match hash_path(algo, filename) {
        Ok(actual) => actual.eq_ignore_ascii_case(expected),
        Err(_) => false,
      };
      if ok {
        if !o.status {
          println!("{filename}: OK");
        }
      } else {
        if !o.status {
          println!("{filename}: FAILED");
        }
        failed += 1;
        status = 1;
      }
    }

    if failed != 0 && !o.status {
      eprintln!("{prog}: WARNING: {failed} of {total} computed checksums did NOT match");
    }
    if total == 0 {
      status = 1;
      eprintln!("{prog}: {list_path}: no checksum lines found");
    }
  }
  status
}

pub fn run(name: &str, argv: &[&str]) -> i32 {
  let algo_is_sha3 = name == "sha3sum";
  let (opts, files) = match parse_args(algo_is_sha3, argv) {
    Ok(v) => v,
    Err(e) => {
      eprintln!("{e}");
      return 1;
    }
  };
  let algo = match name {
    "md5sum" => Algo::Md5,
    "sha1sum" => Algo::Sha1,
    "sha256sum" => Algo::Sha256,
    "sha512sum" => Algo::Sha512,
    "sha3sum" => Algo::Sha3(opts.width),
    _ => unreachable!("only registered for the five hashsum applet names"),
  };

  let result = if opts.check {
    check_files(&algo, name, &files, &opts)
  } else {
    print_hashes(&algo, &files)
  };
  io::stdout().flush().ok();
  result
}

#[allow(dead_code)]
pub fn run_and_exit(argv: &[&str]) -> ! {
  let name = std::path::Path::new(argv.first().map(|s| *s).unwrap_or(""))
    .file_name()
    .and_then(|s| s.to_str())
    .unwrap_or("md5sum");
  std::process::exit(run(name, argv));
}
