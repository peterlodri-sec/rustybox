//! `mount` — Phase 3 idiomatic rewrite (see MIGRATION.md). Talks to the
//! kernel via `nix::mount::mount`, a safe wrapper around mount(2), instead
//! of the transpiled version's raw FFI + hand-rolled `getmntent` parsing.
//!
//! Covers: two-arg `mount [-o OPTS] [-t FSTYPE] SOURCE TARGET` including
//! fstype autodetection by walking `/proc/filesystems` (same source upstream
//! uses), `bind`/`rbind`/`move`/`remount`/`make-{shared,private,slave,
//! unbindable}` (+ recursive variants) either via `-o` or as the shared-
//! subtree fast path, `-a` (mount everything in `/etc/fstab` or `-T FILE`,
//! skipping `noauto`/`swap`/already-mounted entries, tolerating EBUSY),
//! `-O` filtering, and the bare `mount` (no args) listing of `/proc/mounts`.
//!
//! Deliberately out of scope, same spirit as ifconfig's legacy-hardware
//! trim: automatic loop-device attach for `mount image.img dir` (losetup(8)
//! it yourself first, then `mount /dev/loopN dir`), the CIFS UNC / NFS
//! `host:path` fstab shorthand auto-detection, and the `mount.<fstype>`
//! helper-program fallback (already dead code in the transpiled version).

use std::fs;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use nix::errno::Errno;
use nix::mount::{mount, MsFlags};

const DEFAULT_FSTAB: &str = "/etc/fstab";

struct ParsedOpts {
  flags: MsFlags,
  data: Vec<String>,
  noauto: bool,
  swap: bool,
  nofail: bool,
}

fn parse_o_opts(opts: &str) -> ParsedOpts {
  let mut p = ParsedOpts {
    flags: MsFlags::MS_SILENT,
    data: Vec::new(),
    noauto: false,
    swap: false,
    nofail: false,
  };
  for tok in opts.split(',').map(str::trim).filter(|s| !s.is_empty()) {
    let (name, _val) = match tok.split_once('=') {
      Some((n, v)) => (n, Some(v)),
      None => (tok, None),
    };
    match name {
      "loop" | "defaults" | "_netdev" | "comment" | "union" => {}
      "noauto" => p.noauto = true,
      "sw" | "swap" => p.swap = true,
      "user" | "users" => {}
      "nofail" => p.nofail = true,
      "nosuid" => p.flags |= MsFlags::MS_NOSUID,
      "suid" => p.flags &= !MsFlags::MS_NOSUID,
      "dev" => p.flags &= !MsFlags::MS_NODEV,
      "nodev" => p.flags |= MsFlags::MS_NODEV,
      "exec" => p.flags &= !MsFlags::MS_NOEXEC,
      "noexec" => p.flags |= MsFlags::MS_NOEXEC,
      "sync" => p.flags |= MsFlags::MS_SYNCHRONOUS,
      "dirsync" => p.flags |= MsFlags::MS_DIRSYNC,
      "async" => p.flags &= !MsFlags::MS_SYNCHRONOUS,
      "atime" => p.flags &= !MsFlags::MS_NOATIME,
      "noatime" => p.flags |= MsFlags::MS_NOATIME,
      "diratime" => p.flags &= !MsFlags::MS_NODIRATIME,
      "nodiratime" => p.flags |= MsFlags::MS_NODIRATIME,
      "mand" => p.flags |= MsFlags::MS_MANDLOCK,
      "nomand" => p.flags &= !MsFlags::MS_MANDLOCK,
      "relatime" => p.flags |= MsFlags::MS_RELATIME,
      "norelatime" => p.flags &= !MsFlags::MS_RELATIME,
      "strictatime" => p.flags |= MsFlags::MS_STRICTATIME,
      "loud" => p.flags &= !MsFlags::MS_SILENT,
      "rbind" => p.flags |= MsFlags::MS_BIND | MsFlags::MS_REC,
      "bind" => p.flags |= MsFlags::MS_BIND,
      "move" => p.flags |= MsFlags::MS_MOVE,
      "make-shared" => p.flags |= MsFlags::MS_SHARED,
      "make-rshared" => p.flags |= MsFlags::MS_SHARED | MsFlags::MS_REC,
      "make-slave" => p.flags |= MsFlags::MS_SLAVE,
      "make-rslave" => p.flags |= MsFlags::MS_SLAVE | MsFlags::MS_REC,
      "make-private" => p.flags |= MsFlags::MS_PRIVATE,
      "make-rprivate" => p.flags |= MsFlags::MS_PRIVATE | MsFlags::MS_REC,
      "make-unbindable" => p.flags |= MsFlags::MS_UNBINDABLE,
      "make-runbindable" => p.flags |= MsFlags::MS_UNBINDABLE | MsFlags::MS_REC,
      "ro" => p.flags |= MsFlags::MS_RDONLY,
      "rw" => p.flags &= !MsFlags::MS_RDONLY,
      "remount" => p.flags |= MsFlags::MS_REMOUNT,
      _ => p.data.push(tok.to_string()),
    }
  }
  p
}

fn shared_subtree_only(flags: MsFlags) -> bool {
  flags.intersects(
    MsFlags::MS_SHARED | MsFlags::MS_PRIVATE | MsFlags::MS_SLAVE | MsFlags::MS_UNBINDABLE,
  )
}

fn is_root() -> bool {
  nix::unistd::geteuid().is_root()
}

fn autodetect_fstypes() -> Vec<String> {
  let mut out = Vec::new();
  for path in ["/etc/filesystems", "/proc/filesystems"] {
    if let Ok(content) = fs::read_to_string(path) {
      for line in content.lines() {
        let line = line.trim();
        if line.is_empty()
          || line.starts_with('#')
          || line.starts_with('*')
          || line.starts_with("nodev")
        {
          continue;
        }
        let fstype = line.split_whitespace().last().unwrap_or(line);
        if !out.iter().any(|f: &String| f == fstype) {
          out.push(fstype.to_string());
        }
      }
    }
  }
  out
}

// mount(2) itself is the syscall boundary; nix's `mount()` wraps it as an
// ordinary (non-`unsafe`) fn — Rust's safety model is about memory, and this
// call can't corrupt the process's own memory, only kernel/OS state, same
// category as e.g. `std::fs::remove_dir_all`.
fn do_mount(
  source: Option<&str>,
  target: &str,
  fstype: Option<&str>,
  mut flags: MsFlags,
  data: &str,
) -> Result<(), Errno> {
  let data_opt = if data.is_empty() { None } else { Some(data) };
  match mount(source, target, fstype, flags, data_opt) {
    Ok(()) => Ok(()),
    Err(Errno::EACCES) | Err(Errno::EROFS) if !flags.contains(MsFlags::MS_RDONLY) => {
      eprintln!("mount: {target} is write-protected, mounting read-only");
      flags |= MsFlags::MS_RDONLY;
      mount(source, target, fstype, flags, data_opt)
    }
    Err(e) => Err(e),
  }
}

fn singlemount(
  source: &str,
  target: &str,
  fstype: Option<&str>,
  flags: MsFlags,
  data: &str,
) -> io::Result<()> {
  let no_fstype_needed = flags
    .intersects(MsFlags::MS_REMOUNT | MsFlags::MS_BIND | MsFlags::MS_MOVE)
    || shared_subtree_only(flags);
  if fstype.is_some() || no_fstype_needed {
    return do_mount(Some(source), target, fstype, flags, data).map_err(io::Error::from);
  }
  // No -t given: autodetect, same source order upstream uses.
  let mut last_err = None;
  for candidate in autodetect_fstypes() {
    match do_mount(Some(source), target, Some(&candidate), flags, data) {
      Ok(()) => return Ok(()),
      Err(e) => last_err = Some(e),
    }
  }
  Err(io::Error::from(last_err.unwrap_or(Errno::ENODEV)))
}

struct FstabEntry {
  fsname: String,
  dir: String,
  vfstype: String,
  opts: String,
}

fn read_fstab_like(path: &str) -> Vec<FstabEntry> {
  let mut out = Vec::new();
  let Ok(content) = fs::read_to_string(path) else {
    return out;
  };
  for line in content.lines() {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
      continue;
    }
    let fields: Vec<&str> = line.split_whitespace().collect();
    if fields.len() < 3 {
      continue;
    }
    out.push(FstabEntry {
      fsname: fields[0].to_string(),
      dir: fields[1].to_string(),
      vfstype: fields[2].to_string(),
      opts: fields.get(3).copied().unwrap_or("defaults").to_string(),
    });
  }
  out
}

fn normalize(p: &str) -> PathBuf {
  fs::canonicalize(p).unwrap_or_else(|_| PathBuf::from(p))
}

fn already_mounted(dir: &str) -> bool {
  let target = normalize(dir);
  read_fstab_like("/proc/mounts")
    .iter()
    .any(|e| normalize(&e.dir) == target)
}

fn print_mount_listing(fstype_filter: Option<&str>) -> i32 {
  for e in read_fstab_like("/proc/mounts") {
    if fstype_filter.map(|f| f == e.vfstype).unwrap_or(true) {
      println!("{} on {} type {} ({})", e.fsname, e.dir, e.vfstype, e.opts);
    }
  }
  0
}

fn mount_all(fstab_path: &str, fstype_filter: Option<&str>, o_filter: Option<&str>) -> i32 {
  let mut rc = 0;
  for e in read_fstab_like(fstab_path) {
    if let Some(f) = fstype_filter {
      if !f.split(',').any(|c| c == e.vfstype) {
        continue;
      }
    }
    let parsed = parse_o_opts(&e.opts);
    if parsed.noauto || parsed.swap || e.vfstype == "swap" {
      continue;
    }
    if let Some(want) = o_filter {
      let has: Vec<&str> = e.opts.split(',').collect();
      if !want.split(',').all(|w| has.contains(&w)) {
        continue;
      }
    }
    if already_mounted(&e.dir) {
      continue;
    }
    let data = parsed.data.join(",");
    let fstype = if e.vfstype == "auto" {
      None
    } else {
      Some(e.vfstype.as_str())
    };
    match singlemount(&e.fsname, &e.dir, fstype, parsed.flags, &data) {
      Ok(()) => {}
      Err(err) if err.raw_os_error() == Some(Errno::EBUSY as i32) => {}
      Err(err) if parsed.nofail && err.raw_os_error() == Some(Errno::ENOENT as i32) => {}
      Err(err) => {
        eprintln!("mount: mounting {} on {}: {err}", e.fsname, e.dir);
        rc = 1;
      }
    }
  }
  rc
}

struct Args {
  o_opts: String,
  fstype: Option<String>,
  fstab: String,
  o_filter: Option<String>,
  all: bool,
  positional: Vec<String>,
}

fn parse_args(argv: &[&str]) -> Result<Args, String> {
  let mut a = Args {
    o_opts: String::new(),
    fstype: None,
    fstab: DEFAULT_FSTAB.to_string(),
    o_filter: None,
    all: false,
    positional: Vec::new(),
  };
  let push_opt = |o: &mut String, s: &str| {
    if !o.is_empty() {
      o.push(',');
    }
    o.push_str(s);
  };
  let mut it = argv.iter().skip(1).copied();
  while let Some(arg) = it.next() {
    match arg {
      "-o" => push_opt(
        &mut a.o_opts,
        it.next().ok_or("mount: -o requires an argument")?,
      ),
      "-t" => {
        a.fstype = Some(
          it.next()
            .ok_or("mount: -t requires an argument")?
            .to_string(),
        )
      }
      "-T" => {
        a.fstab = it
          .next()
          .ok_or("mount: -T requires an argument")?
          .to_string()
      }
      "-O" => {
        a.o_filter = Some(
          it.next()
            .ok_or("mount: -O requires an argument")?
            .to_string(),
        )
      }
      "-a" => a.all = true,
      "-r" => push_opt(&mut a.o_opts, "ro"),
      "-w" => push_opt(&mut a.o_opts, "rw"),
      "-f" | "-v" | "-n" | "-s" | "-i" => {} // dry-run/verbose/no-mtab/sloppy/no-helper: accepted, no-op
      _ if arg.starts_with("--") => push_opt(&mut a.o_opts, &arg[2..]),
      _ => a.positional.push(arg.to_string()),
    }
  }
  Ok(a)
}

pub fn run(argv: &[&str]) -> i32 {
  let a = match parse_args(argv) {
    Ok(a) => a,
    Err(e) => {
      eprintln!("{e}");
      return 1;
    }
  };
  let parsed_cli_opts = parse_o_opts(&a.o_opts);

  if !is_root() && parsed_cli_opts.flags != MsFlags::MS_SILENT {
    eprintln!("mount: you must be root");
    return 1;
  }

  if shared_subtree_only(parsed_cli_opts.flags) {
    if let Some(target) = a.positional.first() {
      return match do_mount(None, target, None, parsed_cli_opts.flags, "") {
        Ok(()) => 0,
        Err(e) => {
          eprintln!("mount: {target}: {e}");
          1
        }
      };
    }
  }

  if a.positional.is_empty() {
    if a.all {
      return mount_all(&a.fstab, a.fstype.as_deref(), a.o_filter.as_deref());
    }
    return print_mount_listing(a.fstype.as_deref());
  }

  if a.positional.len() >= 2 {
    let data = parsed_cli_opts.data.join(",");
    return match singlemount(
      &a.positional[0],
      &a.positional[1],
      a.fstype.as_deref(),
      parsed_cli_opts.flags,
      &data,
    ) {
      Ok(()) => 0,
      Err(e) => {
        eprintln!(
          "mount: mounting {} on {}: {e}",
          a.positional[0], a.positional[1]
        );
        1
      }
    };
  }

  // remount/bind/move only ever need the one path given (the target); they
  // don't need a source, so skip the fstab lookup entirely.
  if parsed_cli_opts
    .flags
    .intersects(MsFlags::MS_REMOUNT | MsFlags::MS_BIND | MsFlags::MS_MOVE)
  {
    let target = &a.positional[0];
    let data = parsed_cli_opts.data.join(",");
    return match do_mount(
      None,
      target,
      a.fstype.as_deref(),
      parsed_cli_opts.flags,
      &data,
    ) {
      Ok(()) => 0,
      Err(e) => {
        eprintln!("mount: {target}: {e}");
        1
      }
    };
  }

  // Single positional argument: look it up in fstab/mtab by device or
  // mountpoint, same as `mount /some/fstab/entry`.
  let target = Path::new(&a.positional[0]);
  let resolved = fs::canonicalize(target).unwrap_or_else(|_| target.to_path_buf());
  let entries = read_fstab_like(&a.fstab);
  let Some(entry) = entries.iter().rev().find(|e| {
    Path::new(&e.dir).as_os_str().as_bytes() == resolved.as_os_str().as_bytes()
      || e.fsname == a.positional[0]
      || Path::new(&e.dir).as_os_str() == target.as_os_str()
  }) else {
    eprintln!("mount: can't find {} in {}", a.positional[0], a.fstab);
    return 1;
  };
  let base = parse_o_opts(&entry.opts);
  let mut flags = base.flags | parsed_cli_opts.flags;
  if !parsed_cli_opts.data.is_empty() {
    flags |= MsFlags::empty();
  }
  let mut data = base.data;
  data.extend(parsed_cli_opts.data.iter().cloned());
  let fstype = a.fstype.clone().unwrap_or_else(|| entry.vfstype.clone());
  let fstype_opt = if fstype == "auto" {
    None
  } else {
    Some(fstype.as_str())
  };
  match singlemount(
    &entry.fsname,
    &entry.dir,
    fstype_opt,
    flags,
    &data.join(","),
  ) {
    Ok(()) => 0,
    Err(e) => {
      eprintln!("mount: mounting {} on {}: {e}", entry.fsname, entry.dir);
      1
    }
  }
}

pub fn run_and_exit(args: &[&str]) -> ! {
  let code = run(args);
  std::process::exit(code);
}
