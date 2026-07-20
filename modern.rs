//! Modern, memory-safe applet backends.
//!
//! See MIGRATION.md. Each migrated applet routes to an ecosystem crate (uutils,
//! grep-*, ...) when its `modern-<applet>` feature is enabled. `try_run` returns
//! `Some(exit_code)` if a modern backend handled the applet, or `None` to fall
//! through to the transpiled `<applet>_main`.
//!
//! Integration is at the library level (no subprocessing) so everything links
//! into the single multicall binary. `argv[0]` is the applet name, which is the
//! program-name convention uutils' `uumain` expects.

#[cfg(feature = "modern-grep")]
mod grep;
#[cfg(feature = "modern-find")]
mod find;

#[allow(unused_variables)]
pub fn try_run(name: &str, argv: &[&str]) -> Option<i32> {
  match name {
    #[cfg(feature = "modern-grep")]
    "grep" | "egrep" | "fgrep" => Some(grep::run(name, argv)),
    #[cfg(feature = "modern-find")]
    "find" => Some(find::run(argv)),
    #[cfg(feature = "modern-cat")]
    "cat" => Some(uu_cat::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-echo")]
    "echo" => Some(uu_echo::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-ls")]
    "ls" => Some(uu_ls::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-cp")]
    "cp" => Some(uu_cp::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-mv")]
    "mv" => Some(uu_mv::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-rm")]
    "rm" => Some(uu_rm::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-mkdir")]
    "mkdir" => Some(uu_mkdir::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-rmdir")]
    "rmdir" => Some(uu_rmdir::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-ln")]
    "ln" => Some(uu_ln::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-pwd")]
    "pwd" => Some(uu_pwd::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-touch")]
    "touch" => Some(uu_touch::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-true")]
    "true" => Some(uu_true::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-false")]
    "false" => Some(uu_false::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-head")]
    "head" => Some(uu_head::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-tail")]
    "tail" => Some(uu_tail::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-wc")]
    "wc" => Some(uu_wc::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-sort")]
    "sort" => Some(uu_sort::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-uniq")]
    "uniq" => Some(uu_uniq::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-cut")]
    "cut" => Some(uu_cut::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-tr")]
    "tr" => Some(uu_tr::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-chmod")]
    "chmod" => Some(uu_chmod::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-chown")]
    "chown" => Some(uu_chown::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-df")]
    "df" => Some(uu_df::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-du")]
    "du" => Some(uu_du::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-env")]
    "env" => Some(uu_env::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-printenv")]
    "printenv" => Some(uu_printenv::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-date")]
    "date" => Some(uu_date::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-basename")]
    "basename" => Some(uu_basename::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-dirname")]
    "dirname" => Some(uu_dirname::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-readlink")]
    "readlink" => Some(uu_readlink::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-stat")]
    "stat" => Some(uu_stat::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-seq")]
    "seq" => Some(uu_seq::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-sleep")]
    "sleep" => Some(uu_sleep::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-id")]
    "id" => Some(uu_id::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-whoami")]
    "whoami" => Some(uu_whoami::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-yes")]
    "yes" => Some(uu_yes::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-tac")]
    "tac" => Some(uu_tac::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-nl")]
    "nl" => Some(uu_nl::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-tee")]
    "tee" => Some(uu_tee::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-mktemp")]
    "mktemp" => Some(uu_mktemp::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-realpath")]
    "realpath" => Some(uu_realpath::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-nproc")]
    "nproc" => Some(uu_nproc::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-printf")]
    "printf" => Some(uu_printf::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-link")]
    "link" => Some(uu_link::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-unlink")]
    "unlink" => Some(uu_unlink::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-logname")]
    "logname" => Some(uu_logname::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-factor")]
    "factor" => Some(uu_factor::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-timeout")]
    "timeout" => Some(uu_timeout::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-nohup")]
    "nohup" => Some(uu_nohup::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-shuf")]
    "shuf" => Some(uu_shuf::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-nice")]
    "nice" => Some(uu_nice::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-dd")]
    "dd" => Some(uu_dd::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-truncate")]
    "truncate" => Some(uu_truncate::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-fold")]
    "fold" => Some(uu_fold::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-expand")]
    "expand" => Some(uu_expand::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-unexpand")]
    "unexpand" => Some(uu_unexpand::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-comm")]
    "comm" => Some(uu_comm::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-split")]
    "split" => Some(uu_split::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-cksum")]
    "cksum" => Some(uu_cksum::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-paste")]
    "paste" => Some(uu_paste::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-sync")]
    "sync" => Some(uu_sync::uumain(argv.iter().map(std::ffi::OsString::from))),
    #[cfg(feature = "modern-uname")]
    "uname" => Some(uu_uname::uumain(argv.iter().map(std::ffi::OsString::from))),
    _ => None,
  }
}
