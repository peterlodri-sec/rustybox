//! `ip` — Phase 3 idiomatic rewrite (see MIGRATION.md), partial.
//!
//! Unlike the other Phase 3 applets, `ip`'s full CLI surface (addr/link/
//! route/rule/tunnel/neigh, each with show/add/del/change/replace/flush) is
//! large, and the transpiled version already implements all of it correctly
//! via real netlink sockets (busybox's own `libiproute`/`netlink.c`
//! equivalent) — there's no safety bug to fix in the parts we don't cover,
//! just unsafe-FFI-vs-safe-Rust style. So this covers only the read-only,
//! most-frequently-used surface, entirely without ioctls or netlink:
//! `ip addr show`, `ip link show` (via `nix::ifaddrs::getifaddrs`, same as
//! `modern/ifconfig.rs`) and `ip route show` (IPv4 only, via
//! `/proc/net/route`) — plus one narrow, low-risk mutation, `ip link set
//! IFACE up|down`, using the same confined ioctl helper pattern as
//! `modern/ifconfig.rs`.
//!
//! Everything else (`addr add/del/change`, `link set mtu/address/promisc/…`,
//! `route add/del/change`, `rule`/`tunnel`/`neigh`, any selector this file
//! doesn't recognize) returns `None` so the dispatcher in `modern.rs` falls
//! through to the transpiled `ip_main`, which still handles it. This is the
//! same `try_run` fallback contract `modern.rs` already uses per-applet,
//! just exercised per-subcommand here.

use std::fs;
use std::io;
use std::net::Ipv4Addr;
use std::os::fd::{AsRawFd, OwnedFd, RawFd};

use nix::ifaddrs::getifaddrs;
use nix::net::if_::{if_nametoindex, InterfaceFlags};
use nix::sys::socket::{socket, AddressFamily, SockFlag, SockType};

// ---- confined ioctl helpers (mtu read, flags read/write) -------------------
// Same pattern as modern/ifconfig.rs: every unsafe operation lives here.

fn open_ctl_socket() -> io::Result<OwnedFd> {
  socket(
    AddressFamily::Inet,
    SockType::Datagram,
    SockFlag::empty(),
    None,
  )
  .map_err(|e| io::Error::from_raw_os_error(e as i32))
}

fn ifreq_named(name: &str) -> io::Result<libc::ifreq> {
  if name.is_empty() || name.len() >= libc::IF_NAMESIZE {
    return Err(io::Error::new(
      io::ErrorKind::InvalidInput,
      format!("invalid interface name '{name}'"),
    ));
  }
  // SAFETY: ifreq is plain-old-data; all-zero is a valid value.
  let mut ifr: libc::ifreq = unsafe { std::mem::zeroed() };
  for (dst, src) in ifr.ifr_name.iter_mut().zip(name.bytes()) {
    *dst = src as libc::c_char;
  }
  Ok(ifr)
}

fn ioctl_ifreq(fd: RawFd, request: libc::c_ulong, ifr: &mut libc::ifreq) -> io::Result<()> {
  // SAFETY: `ifr` is a fully-initialized `ifreq` owned by the caller; `fd`
  // is a live AF_INET socket. Sole ioctl(2) FFI call in this file.
  let ret = unsafe { libc::ioctl(fd, request as _, ifr as *mut libc::ifreq) };
  if ret < 0 {
    Err(io::Error::last_os_error())
  } else {
    Ok(())
  }
}

fn get_mtu(fd: RawFd, name: &str) -> Option<i32> {
  let mut ifr = ifreq_named(name).ok()?;
  ioctl_ifreq(fd, libc::SIOCGIFMTU, &mut ifr).ok()?;
  // SAFETY: reading the union member SIOCGIFMTU just filled in.
  Some(unsafe { ifr.ifr_ifru.ifru_mtu })
}

fn get_flags(fd: RawFd, name: &str) -> io::Result<i32> {
  let mut ifr = ifreq_named(name)?;
  ioctl_ifreq(fd, libc::SIOCGIFFLAGS, &mut ifr)?;
  Ok(unsafe { ifr.ifr_ifru.ifru_flags as i32 })
}

fn set_flags(fd: RawFd, name: &str, flags: i32) -> io::Result<()> {
  let mut ifr = ifreq_named(name)?;
  unsafe { ifr.ifr_ifru.ifru_flags = flags as _ };
  ioctl_ifreq(fd, libc::SIOCSIFFLAGS, &mut ifr)
}

// ---- link/addr show (fully safe) -------------------------------------------

struct Iface {
  name: String,
  flags: InterfaceFlags,
  mac: Option<[u8; 6]>,
  inet: Vec<(Ipv4Addr, u32)>, // addr, prefixlen
  inet6: Vec<(String, u32)>,
}

fn prefixlen_v4(mask: Option<Ipv4Addr>) -> u32 {
  mask
    .map(|m| u32::from_ne_bytes(m.octets()).count_ones())
    .unwrap_or(32)
}

fn collect_interfaces(only: Option<&str>) -> io::Result<Vec<Iface>> {
  let mut out: Vec<Iface> = Vec::new();
  let addrs = getifaddrs().map_err(|e| io::Error::from_raw_os_error(e as i32))?;
  for ifa in addrs {
    if let Some(want) = only {
      if ifa.interface_name != want {
        continue;
      }
    }
    let idx = match out.iter().position(|i| i.name == ifa.interface_name) {
      Some(i) => i,
      None => {
        out.push(Iface {
          name: ifa.interface_name.clone(),
          flags: ifa.flags,
          mac: None,
          inet: Vec::new(),
          inet6: Vec::new(),
        });
        out.len() - 1
      }
    };
    let entry = &mut out[idx];
    entry.flags = ifa.flags;
    if let Some(addr) = ifa.address.as_ref() {
      if let Some(sin) = addr.as_sockaddr_in() {
        let mask = ifa
          .netmask
          .as_ref()
          .and_then(|m| m.as_sockaddr_in())
          .map(|s| s.ip());
        entry.inet.push((sin.ip(), prefixlen_v4(mask)));
      } else if let Some(sin6) = addr.as_sockaddr_in6() {
        // Netmask isn't exposed in prefixlen form by getifaddrs for v6 in a
        // convenient way here; upstream `ip` shows the real prefix via
        // netlink. We don't have that without one, so this shows /128 for
        // any address without a discoverable mask — acceptable for a
        // read-only display helper, not used for any mutation.
        let plen = ifa
          .netmask
          .as_ref()
          .and_then(|m| m.as_sockaddr_in6())
          .map(|s| s.ip().octets().iter().map(|b| b.count_ones()).sum::<u32>())
          .unwrap_or(128);
        entry.inet6.push((sin6.ip().to_string(), plen));
      } else if let Some(link) = addr.as_link_addr() {
        if let Some(mac) = link.addr() {
          entry.mac = Some(mac);
        }
      }
    }
  }
  Ok(out)
}

fn format_mac(mac: [u8; 6]) -> String {
  mac
    .iter()
    .map(|b| format!("{b:02x}"))
    .collect::<Vec<_>>()
    .join(":")
}

fn flag_names(flags: InterfaceFlags) -> String {
  let table: &[(InterfaceFlags, &str)] = &[
    (InterfaceFlags::IFF_UP, "UP"),
    (InterfaceFlags::IFF_BROADCAST, "BROADCAST"),
    (InterfaceFlags::IFF_LOOPBACK, "LOOPBACK"),
    (InterfaceFlags::IFF_POINTOPOINT, "POINTOPOINT"),
    (InterfaceFlags::IFF_RUNNING, "LOWER_UP"),
    (InterfaceFlags::IFF_NOARP, "NOARP"),
    (InterfaceFlags::IFF_PROMISC, "PROMISC"),
    (InterfaceFlags::IFF_ALLMULTI, "ALLMULTI"),
    (InterfaceFlags::IFF_MULTICAST, "MULTICAST"),
  ];
  table
    .iter()
    .filter(|(bit, _)| flags.contains(*bit))
    .map(|(_, n)| *n)
    .collect::<Vec<_>>()
    .join(",")
}

fn print_link_line(ctl: &OwnedFd, ife: &Iface) {
  let idx = if_nametoindex(ife.name.as_str()).unwrap_or(0);
  let mtu = get_mtu(ctl.as_raw_fd(), &ife.name).unwrap_or(0);
  println!("{idx}: {}: <{}> mtu {mtu}", ife.name, flag_names(ife.flags));
  let encap = if ife.flags.contains(InterfaceFlags::IFF_LOOPBACK) {
    "loopback"
  } else {
    "ether"
  };
  if let Some(mac) = ife.mac {
    println!("    link/{encap} {}", format_mac(mac));
  } else {
    println!("    link/{encap}");
  }
}

fn print_addr_lines(ife: &Iface) {
  for (addr, plen) in &ife.inet {
    let scope = if addr.is_loopback() { "host" } else { "global" };
    println!("    inet {addr}/{plen} scope {scope} {}", ife.name);
  }
  for (addr, plen) in &ife.inet6 {
    let scope = if addr == "::1" { "host" } else { "global" };
    println!("    inet6 {addr}/{plen} scope {scope}");
  }
}

fn cmd_link_show(dev: Option<&str>) -> i32 {
  let ctl = match open_ctl_socket() {
    Ok(c) => c,
    Err(e) => {
      eprintln!("ip: socket: {e}");
      return 1;
    }
  };
  match collect_interfaces(dev) {
    Ok(ifaces) if ifaces.is_empty() && dev.is_some() => {
      eprintln!("ip: Device \"{}\" does not exist.", dev.unwrap());
      1
    }
    Ok(ifaces) => {
      for ife in &ifaces {
        print_link_line(&ctl, ife);
      }
      0
    }
    Err(e) => {
      eprintln!("ip: {e}");
      1
    }
  }
}

fn cmd_addr_show(dev: Option<&str>) -> i32 {
  let ctl = match open_ctl_socket() {
    Ok(c) => c,
    Err(e) => {
      eprintln!("ip: socket: {e}");
      return 1;
    }
  };
  match collect_interfaces(dev) {
    Ok(ifaces) if ifaces.is_empty() && dev.is_some() => {
      eprintln!("ip: Device \"{}\" does not exist.", dev.unwrap());
      1
    }
    Ok(ifaces) => {
      for ife in &ifaces {
        print_link_line(&ctl, ife);
        print_addr_lines(ife);
      }
      0
    }
    Err(e) => {
      eprintln!("ip: {e}");
      1
    }
  }
}

fn cmd_link_set_updown(dev: &str, up: bool) -> i32 {
  let ctl = match open_ctl_socket() {
    Ok(c) => c,
    Err(e) => {
      eprintln!("ip: socket: {e}");
      return 1;
    }
  };
  let fd = ctl.as_raw_fd();
  let cur = match get_flags(fd, dev) {
    Ok(f) => f,
    Err(e) => {
      eprintln!("ip: {dev}: {e}");
      return 1;
    }
  };
  let next = if up {
    cur | libc::IFF_UP | libc::IFF_RUNNING
  } else {
    cur & !libc::IFF_UP
  };
  match set_flags(fd, dev, next) {
    Ok(()) => 0,
    Err(e) => {
      eprintln!("ip: {dev}: {e}");
      1
    }
  }
}

// ---- route show (IPv4 only, /proc/net/route) -------------------------------

fn hex_to_ipv4(hex: &str) -> Option<Ipv4Addr> {
  let v = u32::from_str_radix(hex, 16).ok()?;
  Some(Ipv4Addr::from(v.to_le_bytes()))
}

fn cmd_route_show() -> i32 {
  let Ok(content) = fs::read_to_string("/proc/net/route") else {
    eprintln!("ip: can't read /proc/net/route");
    return 1;
  };
  for line in content.lines().skip(1) {
    let f: Vec<&str> = line.split_whitespace().collect();
    if f.len() < 8 {
      continue;
    }
    let (iface, dest, gw, mask) = (f[0], f[1], f[2], f[7]);
    let dest = hex_to_ipv4(dest).unwrap_or(Ipv4Addr::UNSPECIFIED);
    let gw = hex_to_ipv4(gw).unwrap_or(Ipv4Addr::UNSPECIFIED);
    let plen = prefixlen_v4(hex_to_ipv4(mask));
    if dest.is_unspecified() && plen == 0 {
      if gw.is_unspecified() {
        println!("default dev {iface} scope link");
      } else {
        println!("default via {gw} dev {iface}");
      }
    } else if gw.is_unspecified() {
      println!("{dest}/{plen} dev {iface} scope link");
    } else {
      println!("{dest}/{plen} via {gw} dev {iface}");
    }
  }
  0
}

// ---- dispatch ---------------------------------------------------------------

fn consume_dev(args: &mut std::iter::Peekable<std::vec::IntoIter<&str>>) -> Option<Option<String>> {
  // Accepts `dev IFACE`, or a bare trailing device name (`ip a show eth0`,
  // `ip link show lo` — valid iproute2 grammar for `show` without the `dev`
  // keyword); any other selector (scope/to/label/table/etc) means "not
  // understood" so the caller falls back to transpiled instead of silently
  // ignoring it.
  let mut dev = None;
  while let Some(tok) = args.next() {
    match tok {
      "dev" => dev = args.next().map(str::to_string),
      "show" | "list" | "s" => {}
      other if dev.is_none() => dev = Some(other.to_string()),
      _ => return None,
    }
  }
  Some(dev)
}

pub fn run(argv: &[&str]) -> Option<i32> {
  let mut args: Vec<&str> = argv.iter().skip(1).copied().collect();
  // Strip global family/oneline options we don't need to act on for the
  // read-only paths we cover; anything else falls through.
  args.retain(|a| {
    !matches!(
      *a,
      "-o" | "-oneline" | "-4" | "-6" | "-f" | "-family" | "inet" | "inet6"
    )
  });
  let mut it = args.into_iter().peekable();
  let sub = it.next()?;
  match sub {
    "addr" | "address" | "a" => {
      let dev = consume_dev(&mut it)?;
      Some(cmd_addr_show(dev.as_deref()))
    }
    "link" | "l" => match it.peek().copied() {
      Some("set") => {
        it.next();
        let mut dev = None;
        let mut updown = None;
        while let Some(tok) = it.next() {
          match tok {
            "dev" => dev = it.next().map(str::to_string),
            "up" => updown = Some(true),
            "down" => updown = Some(false),
            other if dev.is_none() => dev = Some(other.to_string()),
            _ => return None,
          }
        }
        match (dev, updown) {
          (Some(d), Some(up)) => Some(cmd_link_set_updown(&d, up)),
          _ => None,
        }
      }
      _ => {
        let dev = consume_dev(&mut it)?;
        Some(cmd_link_show(dev.as_deref()))
      }
    },
    "route" | "r" => {
      if it.next().is_none_or(|t| matches!(t, "show" | "list" | "s")) {
        Some(cmd_route_show())
      } else {
        None
      }
    }
    _ => None,
  }
}
