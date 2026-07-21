use std::ffi::{CString, OsString};
use std::os::unix::ffi::OsStringExt;
use std::process::exit;
use nix::libc;
use nix::unistd::{execvp, Pid};
use std::ptr;

// Command line arguments parsed
struct Args {
    show_min_max: bool,
    pid: Option<libc::pid_t>,
    policy: Option<libc::c_int>,
    priority: Option<libc::c_int>,
    command: Vec<OsString>,
}

fn print_usage() {
    eprintln!("Usage: chrt -m | -p [PRIO] PID | [-rfobi] PRIO PROG [ARGS]");
    exit(1);
}

fn policy_name(pol: libc::c_int) -> &'static str {
    // mask out SCHED_RESET_ON_FORK (0x40000000)
    let pol = pol & !0x40000000;
    match pol {
        libc::SCHED_OTHER => "OTHER",
        libc::SCHED_FIFO => "FIFO",
        libc::SCHED_RR => "RR",
        libc::SCHED_BATCH => "BATCH",
        // libc::SCHED_ISO doesn't exist standardly, usually 4
        4 => "ISO",
        libc::SCHED_IDLE => "IDLE",
        6 => "DEADLINE",
        _ => "UNKNOWN",
    }
}

fn show_min_max(pol: libc::c_int) {
    let min = unsafe { libc::sched_get_priority_min(pol) };
    let max = unsafe { libc::sched_get_priority_max(pol) };
    if min == -1 || max == -1 {
        println!("SCHED_{} not supported", policy_name(pol));
    } else {
        println!("SCHED_{} min/max priority\t: {}/{}", policy_name(pol), min, max);
    }
}

fn parse_args(args: &[&str]) -> Args {
    let mut show_min_max = false;
    let mut pid_flag = false;
    let mut policy = None;
    let mut priority = None;
    let mut pid = None;
    let mut command = Vec::new();

    let mut i = 1;
    while i < args.len() {
        let arg = args[i];
        if arg.starts_with('-') {
            for c in arg.chars().skip(1) {
                match c {
                    'm' => show_min_max = true,
                    'p' => pid_flag = true,
                    'r' => policy = Some(libc::SCHED_RR),
                    'f' => policy = Some(libc::SCHED_FIFO),
                    'o' => policy = Some(libc::SCHED_OTHER),
                    'b' => policy = Some(libc::SCHED_BATCH),
                    'i' => policy = Some(libc::SCHED_IDLE),
                    _ => print_usage(),
                }
            }
            i += 1;
        } else {
            break;
        }
    }

    if show_min_max {
        return Args {
            show_min_max: true,
            pid: None,
            policy: None,
            priority: None,
            command: Vec::new(),
        };
    }

    // Default policy is RR if not specified
    if policy.is_none() {
        policy = Some(libc::SCHED_RR);
    }

    if pid_flag {
        // -p [PRIO] PID
        if i < args.len() {
            // Check if there are two remaining args (PRIO and PID) or one (PID)
            if i + 1 < args.len() {
                priority = Some(args[i].parse().unwrap_or_else(|_| {
                    eprintln!("invalid priority");
                    exit(1);
                }));
                pid = Some(args[i + 1].parse().unwrap_or_else(|_| {
                    eprintln!("invalid pid");
                    exit(1);
                }));
            } else {
                pid = Some(args[i].parse().unwrap_or_else(|_| {
                    eprintln!("invalid pid");
                    exit(1);
                }));
            }
        } else {
            print_usage();
        }
    } else {
        // [-rfobi] PRIO PROG [ARGS]
        if i + 1 < args.len() {
            priority = Some(args[i].parse().unwrap_or_else(|_| {
                eprintln!("invalid priority");
                exit(1);
            }));
            command.extend(args[i + 1..].iter().map(|s| OsString::from(s)));
        } else {
            print_usage();
        }
    }

    Args {
        show_min_max,
        pid,
        policy,
        priority,
        command,
    }
}

pub fn run(args: &[&str]) -> i32 {
    let parsed = parse_args(args);

    if parsed.show_min_max {
        show_min_max(libc::SCHED_OTHER);
        show_min_max(libc::SCHED_FIFO);
        show_min_max(libc::SCHED_RR);
        show_min_max(libc::SCHED_BATCH);
        show_min_max(libc::SCHED_IDLE);
        return 0;
    }

    let policy = parsed.policy.unwrap_or(libc::SCHED_RR);

    if let Some(pid) = parsed.pid {
        if let Some(priority) = parsed.priority {
            // Set priority for PID
            let mut sp = libc::sched_param { sched_priority: priority };
            if unsafe { libc::sched_setscheduler(pid, policy, &mut sp) } < 0 {
                eprintln!("chrt: can't set pid {}'s policy", pid);
                return 1;
            }
        } else {
            // Get priority for PID
            let current_pol = unsafe { libc::sched_getscheduler(pid) };
            if current_pol < 0 {
                eprintln!("chrt: can't get pid {}'s policy", pid);
                return 1;
            }
            let current_pol_masked = current_pol & !0x40000000;
            println!("pid {}'s current scheduling policy: SCHED_{}", pid, policy_name(current_pol_masked));
            
            let mut sp = libc::sched_param { sched_priority: 0 };
            if unsafe { libc::sched_getparam(pid, &mut sp) } != 0 {
                eprintln!("chrt: can't get pid {}'s attributes", pid);
                return 1;
            }
            println!("pid {}'s current scheduling priority: {}", pid, sp.sched_priority);
        }
        return 0;
    }

    if !parsed.command.is_empty() {
        if let Some(priority) = parsed.priority {
            // Set priority for self
            let mut sp = libc::sched_param { sched_priority: priority };
            if unsafe { libc::sched_setscheduler(0, policy, &mut sp) } < 0 {
                eprintln!("chrt: can't set policy");
                return 1;
            }
        }

        let cmd = CString::new(parsed.command[0].clone().into_vec()).unwrap();
        let mut exec_args = Vec::new();
        for arg in &parsed.command {
            exec_args.push(CString::new(arg.clone().into_vec()).unwrap());
        }

        let err = execvp(&cmd, &exec_args).unwrap_err();
        eprintln!("chrt: failed to execute {}: {}", cmd.to_string_lossy(), err);
        return 127;
    }

    0
}
