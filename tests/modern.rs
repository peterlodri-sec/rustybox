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
fn true_false_exit_codes() {
  let t = cmd!(exe(), "true").unchecked().run().unwrap();
  assert_eq!(t.status.code(), Some(0));
  let f = cmd!(exe(), "false").unchecked().run().unwrap();
  assert_eq!(f.status.code(), Some(1));
}
