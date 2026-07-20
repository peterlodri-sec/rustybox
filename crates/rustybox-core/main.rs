//! rustybox-core — the MIT-licensed edition.
//!
//! A memory-safe, multicall busybox-style toolbox built ENTIRELY on permissive
//! (MIT/Apache) crates — the uutils coreutils, ripgrep's search libraries, and
//! rustybox's own dispatch/grep/find code. No BusyBox-derived (GPL) code is
//! compiled here, so this binary is distributable under MIT.
//!
//! Invoke multicall-style: as `cat`, `grep`, ... (argv[0]) via symlinks, or as
//! `rustybox-core <applet> [args...]`.

// Shared MIT source with the full (GPL) crate's modern backends.
#[path = "../../modern/grep.rs"]
mod grep;
#[path = "../../modern/find.rs"]
mod find;

fn main() {
  let raw: Vec<String> = std::env::args().collect();
  let prog = std::path::Path::new(raw.first().map(String::as_str).unwrap_or(""))
    .file_name()
    .and_then(|s| s.to_str())
    .unwrap_or("");

  // If launched by its own name, the applet is argv[1]; else argv[0] (symlink).
  let (name, argv): (String, Vec<&str>) =
    if prog == "rustybox-core" || prog == "rustybox" || prog.is_empty() {
      if raw.len() < 2 {
        eprintln!("usage: rustybox-core <applet> [args...]");
        list_applets();
        std::process::exit(2);
      }
      (raw[1].clone(), raw[1..].iter().map(String::as_str).collect())
    } else {
      (prog.to_string(), raw.iter().map(String::as_str).collect())
    };

  match dispatch(&name, &argv) {
    Some(code) => std::process::exit(code),
    None => {
      eprintln!("rustybox-core: {name}: applet not found");
      std::process::exit(127);
    }
  }
}

fn dispatch(name: &str, argv: &[&str]) -> Option<i32> {
  match name {
    "grep" | "egrep" | "fgrep" => Some(grep::run(name, argv)),
    "find" => Some(find::run(argv)),
    "cat" => Some(uu_cat::uumain(argv.iter().map(std::ffi::OsString::from))),
    "echo" => Some(uu_echo::uumain(argv.iter().map(std::ffi::OsString::from))),
    "ls" => Some(uu_ls::uumain(argv.iter().map(std::ffi::OsString::from))),
    "cp" => Some(uu_cp::uumain(argv.iter().map(std::ffi::OsString::from))),
    "mv" => Some(uu_mv::uumain(argv.iter().map(std::ffi::OsString::from))),
    "rm" => Some(uu_rm::uumain(argv.iter().map(std::ffi::OsString::from))),
    "mkdir" => Some(uu_mkdir::uumain(argv.iter().map(std::ffi::OsString::from))),
    "rmdir" => Some(uu_rmdir::uumain(argv.iter().map(std::ffi::OsString::from))),
    "ln" => Some(uu_ln::uumain(argv.iter().map(std::ffi::OsString::from))),
    "pwd" => Some(uu_pwd::uumain(argv.iter().map(std::ffi::OsString::from))),
    "touch" => Some(uu_touch::uumain(argv.iter().map(std::ffi::OsString::from))),
    "true" => Some(uu_true::uumain(argv.iter().map(std::ffi::OsString::from))),
    "false" => Some(uu_false::uumain(argv.iter().map(std::ffi::OsString::from))),
    "head" => Some(uu_head::uumain(argv.iter().map(std::ffi::OsString::from))),
    "tail" => Some(uu_tail::uumain(argv.iter().map(std::ffi::OsString::from))),
    "wc" => Some(uu_wc::uumain(argv.iter().map(std::ffi::OsString::from))),
    "sort" => Some(uu_sort::uumain(argv.iter().map(std::ffi::OsString::from))),
    "uniq" => Some(uu_uniq::uumain(argv.iter().map(std::ffi::OsString::from))),
    "cut" => Some(uu_cut::uumain(argv.iter().map(std::ffi::OsString::from))),
    "tr" => Some(uu_tr::uumain(argv.iter().map(std::ffi::OsString::from))),
    "chmod" => Some(uu_chmod::uumain(argv.iter().map(std::ffi::OsString::from))),
    "chown" => Some(uu_chown::uumain(argv.iter().map(std::ffi::OsString::from))),
    "df" => Some(uu_df::uumain(argv.iter().map(std::ffi::OsString::from))),
    "du" => Some(uu_du::uumain(argv.iter().map(std::ffi::OsString::from))),
    "env" => Some(uu_env::uumain(argv.iter().map(std::ffi::OsString::from))),
    "printenv" => Some(uu_printenv::uumain(argv.iter().map(std::ffi::OsString::from))),
    "date" => Some(uu_date::uumain(argv.iter().map(std::ffi::OsString::from))),
    "basename" => Some(uu_basename::uumain(argv.iter().map(std::ffi::OsString::from))),
    "dirname" => Some(uu_dirname::uumain(argv.iter().map(std::ffi::OsString::from))),
    "readlink" => Some(uu_readlink::uumain(argv.iter().map(std::ffi::OsString::from))),
    "stat" => Some(uu_stat::uumain(argv.iter().map(std::ffi::OsString::from))),
    "seq" => Some(uu_seq::uumain(argv.iter().map(std::ffi::OsString::from))),
    "sleep" => Some(uu_sleep::uumain(argv.iter().map(std::ffi::OsString::from))),
    "id" => Some(uu_id::uumain(argv.iter().map(std::ffi::OsString::from))),
    "whoami" => Some(uu_whoami::uumain(argv.iter().map(std::ffi::OsString::from))),
    "yes" => Some(uu_yes::uumain(argv.iter().map(std::ffi::OsString::from))),
    "tac" => Some(uu_tac::uumain(argv.iter().map(std::ffi::OsString::from))),
    "nl" => Some(uu_nl::uumain(argv.iter().map(std::ffi::OsString::from))),
    "tee" => Some(uu_tee::uumain(argv.iter().map(std::ffi::OsString::from))),
    "mktemp" => Some(uu_mktemp::uumain(argv.iter().map(std::ffi::OsString::from))),
    "realpath" => Some(uu_realpath::uumain(argv.iter().map(std::ffi::OsString::from))),
    "nproc" => Some(uu_nproc::uumain(argv.iter().map(std::ffi::OsString::from))),
    "printf" => Some(uu_printf::uumain(argv.iter().map(std::ffi::OsString::from))),
    "link" => Some(uu_link::uumain(argv.iter().map(std::ffi::OsString::from))),
    "unlink" => Some(uu_unlink::uumain(argv.iter().map(std::ffi::OsString::from))),
    "logname" => Some(uu_logname::uumain(argv.iter().map(std::ffi::OsString::from))),
    "factor" => Some(uu_factor::uumain(argv.iter().map(std::ffi::OsString::from))),
    "timeout" => Some(uu_timeout::uumain(argv.iter().map(std::ffi::OsString::from))),
    "nohup" => Some(uu_nohup::uumain(argv.iter().map(std::ffi::OsString::from))),
    "shuf" => Some(uu_shuf::uumain(argv.iter().map(std::ffi::OsString::from))),
    "nice" => Some(uu_nice::uumain(argv.iter().map(std::ffi::OsString::from))),
    "dd" => Some(uu_dd::uumain(argv.iter().map(std::ffi::OsString::from))),
    "truncate" => Some(uu_truncate::uumain(argv.iter().map(std::ffi::OsString::from))),
    "fold" => Some(uu_fold::uumain(argv.iter().map(std::ffi::OsString::from))),
    "expand" => Some(uu_expand::uumain(argv.iter().map(std::ffi::OsString::from))),
    "unexpand" => Some(uu_unexpand::uumain(argv.iter().map(std::ffi::OsString::from))),
    "comm" => Some(uu_comm::uumain(argv.iter().map(std::ffi::OsString::from))),
    "split" => Some(uu_split::uumain(argv.iter().map(std::ffi::OsString::from))),
    "cksum" => Some(uu_cksum::uumain(argv.iter().map(std::ffi::OsString::from))),
    "paste" => Some(uu_paste::uumain(argv.iter().map(std::ffi::OsString::from))),
    "sync" => Some(uu_sync::uumain(argv.iter().map(std::ffi::OsString::from))),
    "uname" => Some(uu_uname::uumain(argv.iter().map(std::ffi::OsString::from))),
    "sum" => Some(uu_sum::uumain(argv.iter().map(std::ffi::OsString::from))),
    "base64" => Some(uu_base64::uumain(argv.iter().map(std::ffi::OsString::from))),
    _ => None,
  }
}

fn list_applets() {
  eprintln!("applets: grep egrep fgrep find cat echo ls cp mv rm mkdir rmdir ln pwd touch true false head tail wc sort uniq cut tr chmod chown df du env printenv date basename dirname readlink stat seq sleep id whoami yes tac nl tee mktemp realpath nproc printf link unlink logname factor timeout nohup shuf nice dd truncate fold expand unexpand comm split cksum paste sync uname sum base64");
}
