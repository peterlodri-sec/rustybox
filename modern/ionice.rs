use std::ffi::{CString, OsString};
use std::os::unix::ffi::OsStringExt;
use std::process::exit;
use nix::libc;
use nix::unistd::execvp;

// Constants from linux/ioprio.h
const IOPRIO_WHO_PROCESS: libc::c_int = 1;
const IOPRIO_CLASS_SHIFT: libc::c_int = 13;

const IOPRIO_CLASS_NONE: libc::c_int = 0;
const IOPRIO_CLASS_RT: libc::c_int = 1;
const IOPRIO_CLASS_BE: libc::c_int = 2;
const IOPRIO_CLASS_IDLE: libc::c_int = 3;

fn print_usage() {
    eprintln!("Usage: ionice [-c 1-3] [-n 0-7] [-p PID] [PROG]");
    exit(1);
}

fn class_name(class: libc::c_int) -> &'static str {
    match class {
        IOPRIO_CLASS_NONE => "none",
        IOPRIO_CLASS_RT => "realtime",
        IOPRIO_CLASS_BE => "best-effort",
        IOPRIO_CLASS_IDLE => "idle",
        _ => "unknown",
    }
}

pub fn run(args: &[&str]) -> i32 {
    let mut ioclass = None;
    let mut pri = None;
    let mut pid = None;
    let mut command = Vec::new();

    let mut i = 1;
    while i < args.len() {
        let arg = args[i];
        if arg == "-c" {
            if i + 1 < args.len() {
                ioclass = Some(args[i + 1].parse::<libc::c_int>().unwrap_or_else(|_| {
                    eprintln!("ionice: bad class {}", args[i + 1]);
                    exit(1);
                }));
                if ioclass.unwrap() > 3 {
                    eprintln!("ionice: bad class {}", ioclass.unwrap());
                    exit(1);
                }
                i += 2;
            } else {
                print_usage();
            }
        } else if arg == "-n" {
            if i + 1 < args.len() {
                pri = Some(args[i + 1].parse::<libc::c_int>().unwrap_or_else(|_| {
                    eprintln!("ionice: bad priority {}", args[i + 1]);
                    exit(1);
                }));
                i += 2;
            } else {
                print_usage();
            }
        } else if arg == "-p" {
            if i + 1 < args.len() {
                pid = Some(args[i + 1].parse::<libc::c_int>().unwrap_or_else(|_| {
                    eprintln!("ionice: bad pid {}", args[i + 1]);
                    exit(1);
                }));
                i += 2;
            } else {
                print_usage();
            }
        } else if arg.starts_with('-') {
            print_usage();
        } else {
            break;
        }
    }

    let is_set = ioclass.is_some() || pri.is_some();

    if !is_set {
        // GET mode
        let target_pid = if let Some(p) = pid {
            p
        } else if i < args.len() {
            args[i].parse().unwrap_or_else(|_| {
                eprintln!("ionice: bad pid {}", args[i]);
                exit(1);
            })
        } else {
            0 // self
        };

        let res = unsafe { libc::syscall(libc::SYS_ioprio_get, IOPRIO_WHO_PROCESS, target_pid) };
        if res == -1 {
            eprintln!("ionice: ioprio_get failed");
            return 1;
        }

        let current_class = (res >> IOPRIO_CLASS_SHIFT) & 0x3;
        let current_pri = res & 0xff;

        if current_class == IOPRIO_CLASS_IDLE as libc::c_long {
            println!("{}", class_name(current_class as i32));
        } else {
            println!("{}: prio {}", class_name(current_class as i32), current_pri);
        }
    } else {
        // SET mode
        let mut final_class = ioclass.unwrap_or(IOPRIO_CLASS_NONE);
        let mut final_pri = pri.unwrap_or(0);

        let target_pid = pid.unwrap_or(0);

        let val = final_pri | (final_class << IOPRIO_CLASS_SHIFT);
        if unsafe { libc::syscall(libc::SYS_ioprio_set, IOPRIO_WHO_PROCESS, target_pid, val) } == -1 {
            eprintln!("ionice: ioprio_set failed");
            return 1;
        }

        if i < args.len() {
            command.extend(args[i..].iter().map(|s| OsString::from(s)));
            let cmd = CString::new(command[0].clone().into_vec()).unwrap();
            let mut exec_args = Vec::new();
            for arg in &command {
                exec_args.push(CString::new(arg.clone().into_vec()).unwrap());
            }

            let err = execvp(&cmd, &exec_args).unwrap_err();
            eprintln!("ionice: failed to execute {}: {}", cmd.to_string_lossy(), err);
            return 127;
        }
    }

    0
}

pub fn run_and_exit(argv: &[&str]) -> ! {
    std::process::exit(run(argv))
}
