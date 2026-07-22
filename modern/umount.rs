//! `umount` — Phase 3 idiomatic rewrite (see MIGRATION.md). Uses
//! `nix::mount::umount2`, a safe wrapper around umount2(2), plus ordinary
//! `/proc/mounts` text parsing instead of the transpiled version's
//! `getmntent`/`setmntent` FFI and manual linked-list bookkeeping.
//!
//! Loop-device teardown (`-d`) is out of scope, matching `modern/mount.rs`'s
//! scope trim: this build doesn't auto-attach loop devices on mount, so
//! there's nothing of ours to detach on unmount either.

use std::fs;
use std::io;
use std::path::PathBuf;

use nix::errno::Errno;
use nix::mount::{umount2, MntFlags};

struct MountEntry {
  fsname: String,
  dir: String,
  vfstype: String,
}

fn read_proc_mounts() -> Vec<MountEntry> {
  let mut out = Vec::new();
  let Ok(content) = fs::read_to_string("/proc/mounts") else {
    return out;
  };
  for line in content.lines() {
    let fields: Vec<&str> = line.split_whitespace().collect();
    if fields.len() < 3 {
      continue;
    }
    out.push(MountEntry {
      fsname: fields[0].to_string(),
      dir: fields[1].to_string(),
      vfstype: fields[2].to_string(),
    });
  }
  out
}

fn normalize(p: &str) -> PathBuf {
  fs::canonicalize(p).unwrap_or_else(|_| PathBuf::from(p))
}

fn do_umount(target: &str, flags: MntFlags) -> Result<(), Errno> {
  umount2(target, flags)
}

struct Opts {
  all: bool,
  remount_ro_on_busy: bool,
  lazy: bool,
  force: bool,
  fstype: Option<String>,
}

pub fn run(argv: &[&str]) -> i32 {
  let mut o = Opts {
    all: false,
    remount_ro_on_busy: false,
    lazy: false,
    force: false,
    fstype: None,
  };
  let mut targets = Vec::new();
  let mut it = argv.iter().skip(1).copied();
  while let Some(arg) = it.next() {
    match arg {
      "-a" => o.all = true,
      "-r" => o.remount_ro_on_busy = true,
      "-l" => o.lazy = true,
      "-f" => o.force = true,
      "-d" => {} // loop-device free: out of scope, see module docs
      "-t" => o.fstype = it.next().map(str::to_string),
      _ => targets.push(arg.to_string()),
    }
  }

  let mut flags = MntFlags::empty();
  if o.force {
    flags |= MntFlags::MNT_FORCE;
  }
  if o.lazy {
    flags |= MntFlags::MNT_DETACH;
  }

  let mut status = 0;
  if o.all {
    // Unmount everything, most-recently-mounted first (reverse of
    // /proc/mounts' natural mount order), matching upstream.
    let mounts = read_proc_mounts();
    for e in mounts.iter().rev() {
      if let Some(f) = &o.fstype {
        if !f.split(',').any(|c| c == e.vfstype) {
          continue;
        }
      }
      if e.dir == "/" {
        continue;
      }
      if let Err(err) = unmount_one(&e.dir, flags, o.remount_ro_on_busy, Some(&e.fsname)) {
        eprintln!("umount: can't unmount {}: {err}", e.dir);
        status = 1;
      }
    }
    return status;
  }

  if targets.is_empty() {
    eprintln!("umount: no mount point specified");
    return 1;
  }

  let mounts = read_proc_mounts();
  for t in &targets {
    let resolved = normalize(t);
    let device = mounts
      .iter()
      .rev()
      .find(|e| normalize(&e.dir) == resolved || e.fsname == *t)
      .map(|e| e.fsname.clone());
    if let Err(err) = unmount_one(t, flags, o.remount_ro_on_busy, device.as_deref()) {
      eprintln!("umount: can't unmount {t}: {err}");
      status = 1;
    }
  }
  status
}

fn unmount_one(
  target: &str,
  flags: MntFlags,
  remount_ro_on_busy: bool,
  device: Option<&str>,
) -> io::Result<()> {
  match do_umount(target, flags) {
    Ok(()) => Ok(()),
    Err(Errno::EBUSY) if remount_ro_on_busy => {
      let Some(dev) = device else {
        return Err(io::Error::from(Errno::EBUSY));
      };
      match nix::mount::mount(
        Some(dev),
        target,
        None::<&str>,
        nix::mount::MsFlags::MS_REMOUNT | nix::mount::MsFlags::MS_RDONLY,
        None::<&str>,
      ) {
        Ok(()) => {
          eprintln!("umount: {target} busy - remounted read-only");
          Ok(())
        }
        Err(e) => {
          eprintln!("umount: can't remount {dev} read-only");
          Err(io::Error::from(e))
        }
      }
    }
    Err(e) => Err(io::Error::from(e)),
  }
}

pub fn run_and_exit(args: &[&str]) -> ! {
  let code = run(args);
  std::process::exit(code);
}
