//! Behavior tests for the modern (memory-safe) applet backends.
//! Run with the applets compiled in, e.g.:
//!   cargo test --features modern --test modern
//!
//! Each test invokes the built `rustybox <applet>` and checks output / exit
//! status, exercising the uutils + grep/find backends end to end.

mod common;
use common::exe;
use duct::cmd;

#[test]
fn echo_basic() {
  assert_eq!(cmd!(exe(), "echo", "hello", "world").read().unwrap(), "hello world");
}

#[test]
fn cat_stdin() {
  let out = cmd!(exe(), "cat").stdin_bytes("a\nb\n").read().unwrap();
  assert_eq!(out, "a\nb");
}

#[test]
fn wc_lines() {
  let out = cmd!(exe(), "wc", "-l").stdin_bytes("x\ny\nz\n").read().unwrap();
  assert_eq!(out.trim(), "3");
}

#[test]
fn sort_uniq() {
  let out = cmd!(exe(), "sort")
    .stdin_bytes("b\na\nb\n")
    .pipe(cmd!(exe(), "uniq"))
    .read()
    .unwrap();
  assert_eq!(out, "a\nb");
}

#[test]
fn seq_range() {
  assert_eq!(cmd!(exe(), "seq", "1", "4").read().unwrap(), "1\n2\n3\n4");
}

#[test]
fn head_n() {
  let out = cmd!(exe(), "head", "-n", "2").stdin_bytes("1\n2\n3\n4\n").read().unwrap();
  assert_eq!(out, "1\n2");
}

#[test]
fn tr_delete() {
  let out = cmd!(exe(), "tr", "-d", "aeiou").stdin_bytes("hello").read().unwrap();
  assert_eq!(out, "hll");
}

#[test]
fn cut_fields() {
  let out = cmd!(exe(), "cut", "-d", ":", "-f", "2")
    .stdin_bytes("a:b:c\n")
    .read()
    .unwrap();
  assert_eq!(out, "b");
}

#[test]
fn grep_basic_and_exit() {
  let out = cmd!(exe(), "grep", "an").stdin_bytes("banana\ncherry\n").read().unwrap();
  assert_eq!(out, "banana");

  // No match -> exit 1.
  let status = cmd!(exe(), "grep", "zzz")
    .stdin_bytes("banana\n")
    .unchecked()
    .stdout_null()
    .run()
    .unwrap();
  assert_eq!(status.status.code(), Some(1));
}

#[test]
fn grep_ci_count() {
  let out = cmd!(exe(), "grep", "-ic", "foo")
    .stdin_bytes("Foo\nbar\nFOO\n")
    .read()
    .unwrap();
  assert_eq!(out.trim(), "2");
}

#[test]
fn find_name() {
  let dir = tempfile::tempdir().unwrap();
  let root = dir.path();
  std::fs::write(root.join("a.txt"), "x").unwrap();
  std::fs::create_dir(root.join("sub")).unwrap();
  std::fs::write(root.join("sub/b.txt"), "y").unwrap();
  std::fs::write(root.join("c.log"), "z").unwrap();

  let out = cmd!(exe(), "find", root.to_str().unwrap(), "-name", "*.txt")
    .read()
    .unwrap();
  let mut lines: Vec<&str> = out.lines().collect();
  lines.sort();
  assert_eq!(lines.len(), 2, "got: {out}");
  assert!(lines.iter().all(|l| l.ends_with(".txt")));
}

#[test]
fn find_type_and_maxdepth() {
  let dir = tempfile::tempdir().unwrap();
  let root = dir.path();
  std::fs::write(root.join("top.txt"), "x").unwrap();
  std::fs::create_dir(root.join("d")).unwrap();
  std::fs::write(root.join("d/deep.txt"), "y").unwrap();

  let out = cmd!(exe(), "find", root.to_str().unwrap(), "-maxdepth", "1", "-type", "f")
    .read()
    .unwrap();
  assert_eq!(out.lines().count(), 1, "got: {out}");
}

#[test]
fn timeout_kills_runaway() {
  let status = cmd!(exe(), "timeout", "1", "sleep", "5")
    .unchecked()
    .run()
    .unwrap();
  assert_eq!(status.status.code(), Some(124));
}

#[test]
fn timeout_passes_quick() {
  let out = cmd!(exe(), "timeout", "5", "echo", "ok").read().unwrap();
  assert_eq!(out, "ok");
}

#[test]
fn base64_roundtrip() {
  let enc = cmd!(exe(), "base64").stdin_bytes("hi").read().unwrap();
  assert_eq!(enc, "aGk=");
  let dec = cmd!(exe(), "base64", "-d").stdin_bytes("aGk=").read().unwrap();
  assert_eq!(dec, "hi");
}

#[test]
fn gzip_roundtrip() {
  let out = cmd!(exe(), "gzip")
    .stdin_bytes("compress me please")
    .pipe(cmd!(exe(), "gunzip"))
    .read()
    .unwrap();
  assert_eq!(out, "compress me please");
}

#[test]
fn bzip2_roundtrip() {
  let out = cmd!(exe(), "bzip2")
    .stdin_bytes("bzip payload here")
    .pipe(cmd!(exe(), "bunzip2"))
    .read()
    .unwrap();
  assert_eq!(out, "bzip payload here");
}

#[test]
fn xz_roundtrip() {
  let out = cmd!(exe(), "xz")
    .stdin_bytes("xz payload here")
    .pipe(cmd!(exe(), "unxz"))
    .read()
    .unwrap();
  assert_eq!(out, "xz payload here");
}

#[test]
fn tar_create_list_extract() {
  let src = tempfile::tempdir().unwrap();
  std::fs::write(src.path().join("f.txt"), "tarred content").unwrap();
  let arc = src.path().join("a.tar");
  let arc_s = arc.to_str().unwrap();

  cmd!(exe(), "tar", "-cf", arc_s, "-C", src.path().to_str().unwrap(), "f.txt")
    .run()
    .unwrap();
  let listing = cmd!(exe(), "tar", "-tf", arc_s).read().unwrap();
  assert!(listing.contains("f.txt"), "listing: {listing}");

  let out = tempfile::tempdir().unwrap();
  cmd!(exe(), "tar", "-xf", arc_s, "-C", out.path().to_str().unwrap())
    .run()
    .unwrap();
  assert_eq!(
    std::fs::read_to_string(out.path().join("f.txt")).unwrap(),
    "tarred content"
  );
}

#[test]
fn ifconfig_lo_display() {
  let out = cmd!(exe(), "ifconfig", "lo").read().unwrap();
  assert!(out.contains("lo"), "got: {out}");
  assert!(out.contains("127.0.0.1"), "got: {out}");
  assert!(out.contains("LOOPBACK"), "got: {out}");
}

#[test]
fn ifconfig_all_includes_lo() {
  let out = cmd!(exe(), "ifconfig", "-a").read().unwrap();
  assert!(out.lines().any(|l| l.starts_with("lo ")), "got: {out}");
}

#[test]
fn ifconfig_unknown_iface_errors() {
  let status = cmd!(exe(), "ifconfig", "there-is-no-such-iface")
    .unchecked()
    .stdout_null()
    .stderr_null()
    .run()
    .unwrap();
  assert_ne!(status.status.code(), Some(0));
}

#[test]
fn mountpoint_root_is_mountpoint() {
  let status = cmd!(exe(), "mountpoint", "-q", "/").unchecked().run().unwrap();
  assert_eq!(status.status.code(), Some(0));
}

#[test]
fn mountpoint_regular_dir_is_not() {
  let dir = tempfile::tempdir().unwrap();
  let status = cmd!(exe(), "mountpoint", "-q", dir.path().to_str().unwrap()).unchecked().run().unwrap();
  assert_eq!(status.status.code(), Some(1));
}

#[test]
fn mount_umount_tmpfs_roundtrip() {
  let dir = tempfile::tempdir().unwrap();
  let path = dir.path().to_str().unwrap();
  let status = cmd!(exe(), "mount", "-t", "tmpfs", "tmpfs", path).unchecked().stderr_capture().run().unwrap();
  if status.status.code() != Some(0) {
    eprintln!("SKIPPED: mount tmpfs needs CAP_SYS_ADMIN, not available in this environment");
    return;
  }
  let mp_status = cmd!(exe(), "mountpoint", "-q", path).unchecked().run().unwrap();
  assert_eq!(mp_status.status.code(), Some(0), "tmpfs mount should register as a mountpoint");
  cmd!(exe(), "umount", path).run().unwrap();
  let mp_after = cmd!(exe(), "mountpoint", "-q", path).unchecked().run().unwrap();
  assert_eq!(mp_after.status.code(), Some(1), "should no longer be a mountpoint after umount");
}

#[test]
fn true_false_exit_codes() {
  let t = cmd!(exe(), "true").unchecked().run().unwrap();
  assert_eq!(t.status.code(), Some(0));
  let f = cmd!(exe(), "false").unchecked().run().unwrap();
  assert_eq!(f.status.code(), Some(1));
}
