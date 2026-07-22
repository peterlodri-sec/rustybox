fn main() {
  // glibc splits the DNS resolver (res_mkquery etc.) into libresolv; musl bakes
  // it into libc. Only link it on gnu targets, and pass it as a *final* linker
  // arg (after the object files) so `--as-needed` doesn't drop it before the
  // symbols that need it are seen.
  let env_target = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
  if env_target == "gnu" {
    println!("cargo:rustc-link-arg=-lresolv");
  }

  let sha = std::env::var("RUSTYBOX_GIT_SHA")
    .ok()
    .filter(|s| !s.is_empty())
    .or_else(|| {
      std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    })
    .unwrap_or_else(|| "unknown".into());
  let date = std::env::var("RUSTYBOX_BUILD_DATE")
    .ok()
    .filter(|s| !s.is_empty())
    .unwrap_or_else(|| "unknown".into());
  let args = std::env::var("RUSTYBOX_BUILD_ARGS")
    .ok()
    .unwrap_or_default();
  let target = std::env::var("TARGET").ok().unwrap_or_default();

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
