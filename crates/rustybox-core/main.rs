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
#[path = "../../modern/hashsum.rs"]
mod hashsum;
#[path = "../../modern/compress.rs"]
mod compress;
#[path = "../../modern/tar.rs"]
mod tar;
#[path = "../../modern/flock.rs"]
mod flock;
#[path = "../../modern/setsid.rs"]
mod setsid;
#[cfg(target_os = "linux")]
#[path = "../../modern/chrt.rs"]
mod chrt;
#[cfg(target_os = "linux")]
#[path = "../../modern/ionice.rs"]
mod ionice;
#[path = "../../modern/watch.rs"]
mod watch;
#[path = "../../modern/xargs.rs"]
mod xargs;
#[path = "../../modern/mountpoint.rs"]
mod mountpoint;


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

  if matches!(name.as_str(), "--version" | "-V" | "version") {
    print_version();
    std::process::exit(0);
  }

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
    "true" => Some(0),
    "false" => Some(1),
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
    "printenv" => {
        let mut new_argv = vec![std::ffi::OsString::from("env")];
        new_argv.extend(argv.iter().skip(1).map(std::ffi::OsString::from));
        Some(uu_env::uumain(new_argv.into_iter()))
    },
    "date" => Some(uu_date::uumain(argv.iter().map(std::ffi::OsString::from))),
    "basename" => Some(uu_basename::uumain(argv.iter().map(std::ffi::OsString::from))),
    "dirname" => Some(uu_dirname::uumain(argv.iter().map(std::ffi::OsString::from))),
    "readlink" => Some(uu_readlink::uumain(argv.iter().map(std::ffi::OsString::from))),
    "stat" => Some(uu_stat::uumain(argv.iter().map(std::ffi::OsString::from))),
    "seq" => Some(uu_seq::uumain(argv.iter().map(std::ffi::OsString::from))),
    "sleep" => Some(uu_sleep::uumain(argv.iter().map(std::ffi::OsString::from))),
    "id" => Some(uu_id::uumain(argv.iter().map(std::ffi::OsString::from))),
    "whoami" | "logname" => {
        let new_argv = vec![
            std::ffi::OsString::from("id"),
            std::ffi::OsString::from("-un"),
        ];
        Some(uu_id::uumain(new_argv.into_iter()))
    },
    "yes" => Some(uu_yes::uumain(argv.iter().map(std::ffi::OsString::from))),
    "tac" => Some(uu_tac::uumain(argv.iter().map(std::ffi::OsString::from))),
    "nl" => Some(uu_nl::uumain(argv.iter().map(std::ffi::OsString::from))),
    "tee" => Some(uu_tee::uumain(argv.iter().map(std::ffi::OsString::from))),
    "mktemp" => Some(uu_mktemp::uumain(argv.iter().map(std::ffi::OsString::from))),
    "realpath" => Some(uu_realpath::uumain(argv.iter().map(std::ffi::OsString::from))),
    "nproc" => Some(uu_nproc::uumain(argv.iter().map(std::ffi::OsString::from))),
    "printf" => Some(uu_printf::uumain(argv.iter().map(std::ffi::OsString::from))),
    "link" => {
        let mut new_argv = vec![std::ffi::OsString::from("ln")];
        new_argv.extend(argv.iter().skip(1).map(std::ffi::OsString::from));
        Some(uu_ln::uumain(new_argv.into_iter()))
    },
    "unlink" => {
        let mut new_argv = vec![std::ffi::OsString::from("rm")];
        new_argv.extend(argv.iter().skip(1).map(std::ffi::OsString::from));
        Some(uu_rm::uumain(new_argv.into_iter()))
    },
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
    "sync" => {
        // rustybox-core doesn't link nix right now, actually we can just call libc directly
        unsafe { let _ = std::ffi::CStr::from_ptr("sync".as_ptr() as *const _); } // hack to not pull nix here since rustybox-core is mostly pure
        // wait, nix is used in modern.rs, but rustybox-core doesn't have it. Let's just use libc.
        unsafe { libc::sync(); }
        Some(0)
    },
    "uname" | "arch" => Some(uu_uname::uumain(argv.iter().map(std::ffi::OsString::from))),
    "sum" => Some(uu_sum::uumain(argv.iter().map(std::ffi::OsString::from))),
    "base64" => Some(uu_base64::uumain(argv.iter().map(std::ffi::OsString::from))),
    "md5sum" | "sha1sum" | "sha256sum" | "sha512sum" | "sha3sum" => Some(hashsum::run(name, argv)),
    "gzip" | "gunzip" | "zcat" => Some(compress::run(name, argv, &compress::GZIP)),
    "bzip2" | "bunzip2" | "bzcat" => Some(compress::run(name, argv, &compress::BZIP2)),
    "xz" | "unxz" | "xzcat" => Some(compress::run(name, argv, &compress::XZ)),
    "tar" => Some(tar::run(argv)),
    "flock" => Some(flock::run(argv)),
    "setsid" => Some(setsid::run(argv)),
    #[cfg(target_os = "linux")]
    "chrt" => Some(chrt::run(argv)),
    #[cfg(target_os = "linux")]
    "ionice" => Some(ionice::run(argv)),
    "watch" => Some(watch::run(argv)),
    "xargs" => Some(xargs::run(argv)),
    "mountpoint" => Some(mountpoint::run(argv)),
    _ => None,
  }
}

fn print_version() {
  println!("rustybox-core {} (MIT edition)", env!("CARGO_PKG_VERSION"));
  println!("commit:   {}", env!("RB_GIT_SHA"));
  println!("built:    {}", env!("RB_BUILD_DATE"));
  println!("target:   {}", env!("RB_TARGET"));
  println!("args:     {}", env!("RB_BUILD_ARGS"));
  println!("repo:     https://github.com/peterlodri-sec/rustybox");
  println!("site:     https://rustybox.io");
  println!("backends: uutils + ripgrep libs · memory-safe, permissive-licensed");
}

fn list_applets() {
  eprintln!("applets: grep egrep fgrep find cat echo ls cp mv rm mkdir rmdir ln pwd touch true false head tail wc sort uniq cut tr chmod chown df du env printenv date basename dirname readlink stat seq sleep id whoami yes tac nl tee mktemp realpath nproc printf link unlink logname factor timeout nohup shuf nice dd truncate fold expand unexpand comm split cksum paste sync uname arch sum base64 md5sum sha1sum sha256sum sha512sum sha3sum gzip gunzip zcat bzip2 bunzip2 bzcat xz unxz xzcat tar flock setsid chrt ionice watch xargs mountpoint");
}
