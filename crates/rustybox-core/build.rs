//! Bake build provenance into rustybox-core (surfaced by `--version`).
//! Values come from the environment in CI (release.yml) with a git fallback,
//! so a plain `cargo build` still records the commit.
use std::process::Command;

fn env(k: &str) -> Option<String> {
  std::env::var(k).ok().filter(|s| !s.is_empty())
}

fn main() {
  let sha = env("RUSTYBOX_GIT_SHA")
    .or_else(|| {
      Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    })
    .unwrap_or_else(|| "unknown".into());
  let date = env("RUSTYBOX_BUILD_DATE").unwrap_or_else(|| "unknown".into());
  let args = env("RUSTYBOX_BUILD_ARGS").unwrap_or_default();
  let target = env("TARGET").unwrap_or_default();

  println!("cargo:rustc-env=RB_GIT_SHA={sha}");
  println!("cargo:rustc-env=RB_BUILD_DATE={date}");
  println!("cargo:rustc-env=RB_BUILD_ARGS={args}");
  println!("cargo:rustc-env=RB_TARGET={target}");
  for k in [
    "RUSTYBOX_GIT_SHA",
    "RUSTYBOX_BUILD_DATE",
    "RUSTYBOX_BUILD_ARGS",
  ] {
    println!("cargo:rerun-if-env-changed={k}");
  }
}
