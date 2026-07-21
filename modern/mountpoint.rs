//! `mountpoint` — Phase 3 idiomatic rewrite (see MIGRATION.md). Entirely
//! `unsafe`-free: it's just `stat`/`lstat` via `std::fs` plus the standard
//! glibc major/minor bit-decoding formula applied to the `st_dev`/`st_rdev`
//! `u64` that `MetadataExt` already gives us, instead of the transpiled
//! version's raw `lstat`/`gnu_dev_major`/`gnu_dev_minor` FFI.

use std::fs;
use std::os::unix::fs::{FileTypeExt, MetadataExt};

const S_IFBLK: u32 = 0o60000;

fn major(dev: u64) -> u32 {
  (((dev >> 8) & 0xfff) | ((dev >> 32) & !0xfff)) as u32
}

fn minor(dev: u64) -> u32 {
  ((dev & 0xff) | ((dev >> 12) & !0xff)) as u32
}

/// Device backing the mount at `dir`, per `/proc/mounts` — "UNKNOWN" if none
/// matches (anonymous superblock, e.g. btrfs subvolumes), matching upstream.
fn device_for_mountpoint(dir: &str) -> String {
  let resolved = fs::canonicalize(dir).unwrap_or_else(|_| dir.into());
  fs::read_to_string("/proc/mounts")
    .ok()
    .and_then(|content| {
      content.lines().rev().find_map(|line| {
        let mut fields = line.split_whitespace();
        let fsname = fields.next()?;
        let mnt_dir = fields.next()?;
        (fs::canonicalize(mnt_dir).unwrap_or_else(|_| mnt_dir.into()) == resolved).then(|| fsname.to_string())
      })
    })
    .unwrap_or_else(|| "UNKNOWN".to_string())
}

struct Opts {
  quiet: bool,
  devno: bool,
  blockdev_devno: bool,
  devname: bool,
}

pub fn run(argv: &[&str]) -> i32 {
  let mut o = Opts { quiet: false, devno: false, blockdev_devno: false, devname: false };
  let mut arg = None;
  for a in argv.iter().skip(1).copied() {
    match a {
      "-q" => o.quiet = true,
      "-d" => o.devno = true,
      "-x" => o.blockdev_devno = true,
      "-n" => o.devname = true,
      _ => arg = Some(a),
    }
  }
  let Some(path) = arg else {
    eprintln!("mountpoint: no directory specified");
    return 1;
  };

  if o.blockdev_devno {
    return match fs::metadata(path) {
      Ok(meta) if meta.file_type().is_block_device() => {
        println!("{}:{}", major(meta.rdev()), minor(meta.rdev()));
        0
      }
      Ok(_) => {
        if !o.quiet {
          eprintln!("mountpoint: {path}: not a block device");
        }
        1
      }
      Err(e) => {
        if !o.quiet {
          eprintln!("mountpoint: {path}: {e}");
        }
        1
      }
    };
  }

  let meta = match fs::symlink_metadata(path) {
    Ok(m) => m,
    Err(e) => {
      if !o.quiet {
        eprintln!("mountpoint: {path}: {e}");
      }
      return 1;
    }
  };
  if !meta.is_dir() {
    if !o.quiet {
      eprintln!("mountpoint: {path}: Not a directory");
    }
    return 1;
  }

  let parent = format!("{}/..", path.trim_end_matches('/'));
  let parent_meta = match fs::metadata(&parent) {
    Ok(m) => m,
    Err(e) => {
      if !o.quiet {
        eprintln!("mountpoint: {path}: {e}");
      }
      return 1;
    }
  };
  let is_mountpoint = meta.dev() != parent_meta.dev() || meta.ino() == parent_meta.ino();

  if o.devno {
    println!("{}:{}", major(meta.dev()), minor(meta.dev()));
  }
  if o.devname {
    println!("{} {path}", device_for_mountpoint(path));
  }
  if !o.devno && !o.devname {
    println!("{path} is {}a mountpoint", if is_mountpoint { "" } else { "not " });
  }
  i32::from(!is_mountpoint)
}

pub fn run_and_exit(args: &[&str]) -> ! {
  let code = run(args);
  std::process::exit(code);
}

