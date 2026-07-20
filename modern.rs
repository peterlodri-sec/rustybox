//! Modern, memory-safe applet backends.
//!
//! See MIGRATION.md. Each migrated applet routes to an ecosystem crate (uutils,
//! grep-*, …) when its `modern-<applet>` feature is enabled. `try_run` returns
//! `Some(exit_code)` if a modern backend handled the applet, or `None` to fall
//! through to the transpiled `<applet>_main`.
//!
//! Integration is at the library level (no subprocessing) so everything links
//! into the single multicall binary. `argv[0]` is the applet name, which is the
//! program-name convention uutils' `uumain` expects.

#[allow(unused_variables)]
pub fn try_run(name: &str, argv: &[&str]) -> Option<i32> {
  match name {
    #[cfg(feature = "modern-cat")]
    "cat" => Some(uu_cat::uumain(
      argv.iter().map(std::ffi::OsString::from),
    )),
    _ => None,
  }
}
