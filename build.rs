fn main() {
  // glibc splits the DNS resolver (res_mkquery etc.) into libresolv; musl bakes
  // it into libc. Only link it on gnu targets, and pass it as a *final* linker
  // arg (after the object files) so `--as-needed` doesn't drop it before the
  // symbols that need it are seen.
  let env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
  if env == "gnu" {
    println!("cargo:rustc-link-arg=-lresolv");
  }
}
