//! `ifconfig` — Phase 3 idiomatic rewrite (see MIGRATION.md). No drop-in
//! crate exists for network interface configuration, so this talks to the
//! kernel directly instead of going through the transpiled version's raw
//! pointer/ioctl FFI.
//!
//! Display uses `nix::ifaddrs::getifaddrs`, a safe wrapper around
//! getifaddrs(3), so the whole read path is unsafe-free. Setting flags or
//! addresses still requires ioctl(2) — there is no safe wrapper for that —
//! so it is confined to a handful of small helpers (`ioctl_ifreq` and the
//! `ifr_get_*`/`ifr_set_*` union accessors) instead of being threaded
//! through the whole file as raw pointer arithmetic the way the transpiled
//! version does it.
//!
//! Scope: the common IPv4 surface (address, netmask, broadcast,
//! pointopoint, dstaddr, hw ether, mtu, metric, txqueuelen, up/down, arp,
//! promisc, allmulti, multicast, dynamic, trailers) plus read-only IPv6
//! display. Deliberately out of scope, same as upstream BusyBox admits to
//! ("Still missing: media, tunnel"): IPv6 add/del, `hw infiniband`, and the
//! legacy SLIP/ISA-era options (`mem_start`, `io_addr`, `irq`, `keepalive`,
//! `outfill`) — dead hardware classes that predate any interface this crate
//! needs to support.

use std::io;
use std::net::Ipv4Addr;
use std::os::fd::{AsRawFd, OwnedFd, RawFd};
use std::str::FromStr;

use nix::ifaddrs::getifaddrs;
use nix::net::if_::InterfaceFlags;
use nix::sys::socket::{socket, AddressFamily, SockFlag, SockType};

const IF_NAMESIZE: usize = libc::IF_NAMESIZE;

// ---- ioctl boundary -------------------------------------------------------
//
// Every unsafe operation in this applet lives in this section. Everything
// above and below it is ordinary, checked Rust: no manual pointer offsets,
// no raw strcmp/strncpy, no wrapping-cast bitmasks.

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
  if name.is_empty() || name.len() >= IF_NAMESIZE {
    return Err(io::Error::new(
      io::ErrorKind::InvalidInput,
      format!("invalid interface name '{name}'"),
    ));
  }
  // SAFETY: ifreq is a plain-old-data C struct; all-zero is a valid value.
  let mut ifr: libc::ifreq = unsafe { std::mem::zeroed() };
  for (dst, src) in ifr.ifr_name.iter_mut().zip(name.bytes()) {
    *dst = src as libc::c_char;
  }
  Ok(ifr)
}

fn ioctl_ifreq(fd: RawFd, request: libc::c_ulong, ifr: &mut libc::ifreq) -> io::Result<()> {
  // SAFETY: `ifr` is a fully-initialized `ifreq` owned by the caller for the
  // duration of this call, and `fd` is a live AF_INET socket. This is the
  // single ioctl(2) FFI call in the applet; every caller builds `ifr`
  // through the safe helpers above/below.
  let ret = unsafe { libc::ioctl(fd, request as _, ifr as *mut libc::ifreq) };
  if ret < 0 {
    Err(io::Error::last_os_error())
  } else {
    Ok(())
  }
}

fn ifr_get_flags(ifr: &libc::ifreq) -> i32 {
  // SAFETY: reading the union member the kernel just filled in via
  // SIOCGIFFLAGS immediately before this is called.
  unsafe { ifr.ifr_ifru.ifru_flags as i32 }
}

fn ifr_set_flags(ifr: &mut libc::ifreq, flags: i32) {
  // SAFETY: writing the union member that SIOCSIFFLAGS reads.
  unsafe { ifr.ifr_ifru.ifru_flags = flags as _ };
}

fn ifr_set_mtu(ifr: &mut libc::ifreq, mtu: i32) {
  unsafe { ifr.ifr_ifru.ifru_mtu = mtu };
}

fn ifr_set_metric(ifr: &mut libc::ifreq, metric: i32) {
  unsafe { ifr.ifr_ifru.ifru_metric = metric };
}

fn ifr_set_ivalue(ifr: &mut libc::ifreq, v: i32) {
  // SIOCSIFTXQLEN reads a plain `int` out of the same union slot as
  // ifru_mtu (the kernel calls it ifr_qlen; libc's binding just doesn't
  // give that union member its own name).
  unsafe { ifr.ifr_ifru.ifru_mtu = v };
}

fn sockaddr_in(addr: Ipv4Addr) -> libc::sockaddr_in {
  libc::sockaddr_in {
    sin_family: libc::AF_INET as libc::sa_family_t,
    sin_port: 0,
    sin_addr: libc::in_addr {
      s_addr: u32::from_ne_bytes(addr.octets()),
    },
    sin_zero: [0; 8],
  }
}

fn ifr_set_sockaddr(ifr: &mut libc::ifreq, addr: Ipv4Addr) {
  let sin = sockaddr_in(addr);
  // SAFETY: `sockaddr_in` and `sockaddr` are both C structs the kernel
  // treats as interchangeable via `sa_family`; this mirrors the standard
  // idiom for filling ifr_ifru.ifru_addr/dstaddr/broadaddr/netmask.
  unsafe {
    std::ptr::write(
      &mut ifr.ifr_ifru.ifru_addr as *mut libc::sockaddr as *mut libc::sockaddr_in,
      sin,
    );
  }
}

fn ifr_set_hwaddr(ifr: &mut libc::ifreq, mac: [u8; 6]) {
  let mut sa: libc::sockaddr = unsafe { std::mem::zeroed() };
  sa.sa_family = libc::ARPHRD_ETHER as libc::sa_family_t;
  for (dst, src) in sa.sa_data.iter_mut().zip(mac) {
    *dst = src as libc::c_char;
  }
  unsafe { ifr.ifr_ifru.ifru_hwaddr = sa };
}

// ---- argument parsing (fully safe) ----------------------------------------

fn parse_ipv4(s: &str) -> Option<Ipv4Addr> {
  if s == "default" {
    return Some(Ipv4Addr::UNSPECIFIED);
  }
  Ipv4Addr::from_str(s).ok()
}

fn parse_mac(s: &str) -> Option<[u8; 6]> {
  let mut mac = [0u8; 6];
  let parts: Vec<&str> = s.split(':').collect();
  if parts.len() != 6 {
    return None;
  }
  for (dst, part) in mac.iter_mut().zip(parts) {
    *dst = u8::from_str_radix(part, 16).ok()?;
  }
  Some(mac)
}

struct Ctl {
  fd: OwnedFd,
}

impl Ctl {
  fn open() -> io::Result<Self> {
    Ok(Self {
      fd: open_ctl_socket()?,
    })
  }

  fn get_flags(&self, name: &str) -> io::Result<i32> {
    let mut ifr = ifreq_named(name)?;
    ioctl_ifreq(self.fd.as_raw_fd(), libc::SIOCGIFFLAGS, &mut ifr)?;
    Ok(ifr_get_flags(&ifr))
  }

  fn set_flags(&self, name: &str, flags: i32) -> io::Result<()> {
    let mut ifr = ifreq_named(name)?;
    ifr_set_flags(&mut ifr, flags);
    ioctl_ifreq(self.fd.as_raw_fd(), libc::SIOCSIFFLAGS, &mut ifr)
  }

  fn toggle_flag(&self, name: &str, bit: i32, set: bool) -> io::Result<()> {
    let cur = self.get_flags(name)?;
    let next = if set { cur | bit } else { cur & !bit };
    self.set_flags(name, next)
  }

  fn set_addr(&self, name: &str, request: libc::c_ulong, addr: Ipv4Addr) -> io::Result<()> {
    let mut ifr = ifreq_named(name)?;
    ifr_set_sockaddr(&mut ifr, addr);
    ioctl_ifreq(self.fd.as_raw_fd(), request, &mut ifr)
  }

  fn set_mtu(&self, name: &str, mtu: i32) -> io::Result<()> {
    let mut ifr = ifreq_named(name)?;
    ifr_set_mtu(&mut ifr, mtu);
    ioctl_ifreq(self.fd.as_raw_fd(), libc::SIOCSIFMTU, &mut ifr)
  }

  fn set_metric(&self, name: &str, metric: i32) -> io::Result<()> {
    let mut ifr = ifreq_named(name)?;
    ifr_set_metric(&mut ifr, metric);
    ioctl_ifreq(self.fd.as_raw_fd(), libc::SIOCSIFMETRIC, &mut ifr)
  }

  fn set_txqueuelen(&self, name: &str, len: i32) -> io::Result<()> {
    let mut ifr = ifreq_named(name)?;
    ifr_set_ivalue(&mut ifr, len);
    ioctl_ifreq(self.fd.as_raw_fd(), libc::SIOCSIFTXQLEN, &mut ifr)
  }

  fn set_hwaddr(&self, name: &str, mac: [u8; 6]) -> io::Result<()> {
    let mut ifr = ifreq_named(name)?;
    ifr_set_hwaddr(&mut ifr, mac);
    ioctl_ifreq(self.fd.as_raw_fd(), libc::SIOCSIFHWADDR, &mut ifr)
  }
}

fn set_interface(name: &str, opts: &[&str]) -> i32 {
  let ctl = match Ctl::open() {
    Ok(c) => c,
    Err(e) => {
      eprintln!("ifconfig: socket: {e}");
      return 1;
    }
  };

  let mut i = 0;
  let mut addr_set = false;
  while i < opts.len() {
    let arg = opts[i];
    let (bare, invert) = match arg.strip_prefix('-') {
      Some(rest) => (rest, true),
      None => (arg, false),
    };

    macro_rules! next_val {
      ($what:expr) => {{
        i += 1;
        match opts.get(i) {
          Some(v) => *v,
          None => {
            eprintln!("ifconfig: option '{}' requires {}", arg, $what);
            return 1;
          }
        }
      }};
    }
    macro_rules! fail {
      ($e:expr) => {{
        eprintln!("ifconfig: SIOC ioctl for '{name}': {}", $e);
        return 1;
      }};
    }

    let result = match bare {
      "up" => ctl.toggle_flag(name, libc::IFF_UP | libc::IFF_RUNNING, true),
      "down" => ctl.toggle_flag(name, libc::IFF_UP, false),
      "arp" => ctl.toggle_flag(name, libc::IFF_NOARP, invert),
      "trailers" => ctl.toggle_flag(name, libc::IFF_NOTRAILERS, invert),
      "promisc" => ctl.toggle_flag(name, libc::IFF_PROMISC, !invert),
      "allmulti" => ctl.toggle_flag(name, libc::IFF_ALLMULTI, !invert),
      "multicast" => ctl.toggle_flag(name, libc::IFF_MULTICAST, !invert),
      "dynamic" => ctl.toggle_flag(name, libc::IFF_DYNAMIC, !invert),
      "netmask" => {
        let v = next_val!("an address");
        match parse_ipv4(v) {
          Some(a) => ctl.set_addr(name, libc::SIOCSIFNETMASK, a),
          None => {
            eprintln!("ifconfig: bad netmask '{v}'");
            return 1;
          }
        }
      }
      "broadcast" => {
        if invert {
          ctl.toggle_flag(name, libc::IFF_BROADCAST, false)
        } else {
          let v = next_val!("an address");
          match parse_ipv4(v) {
            Some(a) => ctl
              .set_addr(name, libc::SIOCSIFBRDADDR, a)
              .and_then(|_| ctl.toggle_flag(name, libc::IFF_BROADCAST, true)),
            None => {
              eprintln!("ifconfig: bad broadcast address '{v}'");
              return 1;
            }
          }
        }
      }
      "pointopoint" => {
        if invert {
          ctl.toggle_flag(name, libc::IFF_POINTOPOINT, false)
        } else {
          let v = next_val!("an address");
          match parse_ipv4(v) {
            Some(a) => ctl
              .set_addr(name, libc::SIOCSIFDSTADDR, a)
              .and_then(|_| ctl.toggle_flag(name, libc::IFF_POINTOPOINT, true)),
            None => {
              eprintln!("ifconfig: bad pointopoint address '{v}'");
              return 1;
            }
          }
        }
      }
      "dstaddr" => {
        let v = next_val!("an address");
        match parse_ipv4(v) {
          Some(a) => ctl.set_addr(name, libc::SIOCSIFDSTADDR, a),
          None => {
            eprintln!("ifconfig: bad dstaddr '{v}'");
            return 1;
          }
        }
      }
      "mtu" => {
        let v = next_val!("a number");
        match v.parse::<i32>() {
          Ok(n) => ctl.set_mtu(name, n),
          Err(_) => {
            eprintln!("ifconfig: bad mtu '{v}'");
            return 1;
          }
        }
      }
      "metric" => {
        let v = next_val!("a number");
        match v.parse::<i32>() {
          Ok(n) => ctl.set_metric(name, n),
          Err(_) => {
            eprintln!("ifconfig: bad metric '{v}'");
            return 1;
          }
        }
      }
      "txqueuelen" => {
        let v = next_val!("a number");
        match v.parse::<i32>() {
          Ok(n) => ctl.set_txqueuelen(name, n),
          Err(_) => {
            eprintln!("ifconfig: bad txqueuelen '{v}'");
            return 1;
          }
        }
      }
      "hw" => {
        let class = next_val!("a hardware class");
        if class != "ether" {
          eprintln!("ifconfig: hw class '{class}' not supported (only 'ether')");
          return 1;
        }
        let v = next_val!("a hardware address");
        match parse_mac(v) {
          Some(mac) => ctl.set_hwaddr(name, mac),
          None => {
            eprintln!("ifconfig: bad hw ether address '{v}'");
            return 1;
          }
        }
      }
      "add" | "del" => {
        eprintln!("ifconfig: IPv6 'add'/'del' are not supported by this build");
        return 1;
      }
      "mem_start" | "io_addr" | "irq" | "keepalive" | "outfill" => {
        eprintln!("ifconfig: '{bare}' targets legacy SLIP/ISA hardware and is not supported");
        return 1;
      }
      _ => {
        // Bare token with no keyword: the interface address, IPv4-only.
        match parse_ipv4(arg) {
          Some(a) if !addr_set => {
            addr_set = true;
            ctl.set_addr(name, libc::SIOCSIFADDR, a)
          }
          _ => {
            eprintln!("ifconfig: unrecognized argument '{arg}'");
            return 1;
          }
        }
      }
    };
    if let Err(e) = result {
      fail!(e);
    }
    i += 1;
  }
  0
}

// ---- display ---------------------------------------------------------------

struct Iface {
  name: String,
  flags: InterfaceFlags,
  mac: Option<[u8; 6]>,
  inet: Option<(Ipv4Addr, Option<Ipv4Addr>, Option<Ipv4Addr>)>, // addr, netmask, broadcast
  inet6: Vec<String>,
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
    let entry = match out.iter_mut().find(|i| i.name == ifa.interface_name) {
      Some(e) => e,
      None => {
        out.push(Iface {
          name: ifa.interface_name.clone(),
          flags: ifa.flags,
          mac: None,
          inet: None,
          inet6: Vec::new(),
        });
        out.last_mut().unwrap()
      }
    };
    entry.flags = ifa.flags;
    if let Some(addr) = ifa.address.as_ref() {
      if let Some(sin) = addr.as_sockaddr_in() {
        let netmask = ifa
          .netmask
          .as_ref()
          .and_then(|a| a.as_sockaddr_in())
          .map(|s| s.ip());
        let broadcast = ifa
          .broadcast
          .as_ref()
          .and_then(|a| a.as_sockaddr_in())
          .map(|s| s.ip());
        entry.inet = Some((sin.ip(), netmask, broadcast));
      } else if let Some(sin6) = addr.as_sockaddr_in6() {
        entry.inet6.push(sin6.ip().to_string());
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
    .map(|b| format!("{b:02X}"))
    .collect::<Vec<_>>()
    .join(":")
}

fn print_iface(ife: &Iface) {
  let loopback = ife.flags.contains(InterfaceFlags::IFF_LOOPBACK);
  print!(
    "{:<9} Link encap:{}",
    ife.name,
    if loopback {
      "Local Loopback"
    } else {
      "Ethernet"
    }
  );
  // Loopback/tunnel devices report an all-zero link address; matches
  // upstream's behavior of only printing a HWaddr for real ethernet-class
  // hardware.
  if let Some(mac) = ife.mac {
    if !loopback && mac != [0u8; 6] {
      print!("  HWaddr {}", format_mac(mac));
    }
  }
  println!();
  if let Some((addr, netmask, broadcast)) = ife.inet {
    print!("          inet addr:{addr}");
    if let Some(b) = broadcast {
      print!("  Bcast:{b}");
    }
    if let Some(m) = netmask {
      print!("  Mask:{m}");
    }
    println!();
  }
  for a6 in &ife.inet6 {
    println!("          inet6 addr: {a6}");
  }

  let mut flags = Vec::new();
  for (bit, name) in [
    (InterfaceFlags::IFF_UP, "UP"),
    (InterfaceFlags::IFF_BROADCAST, "BROADCAST"),
    (InterfaceFlags::IFF_LOOPBACK, "LOOPBACK"),
    (InterfaceFlags::IFF_POINTOPOINT, "POINTOPOINT"),
    (InterfaceFlags::IFF_RUNNING, "RUNNING"),
    (InterfaceFlags::IFF_NOARP, "NOARP"),
    (InterfaceFlags::IFF_PROMISC, "PROMISC"),
    (InterfaceFlags::IFF_ALLMULTI, "ALLMULTI"),
    (InterfaceFlags::IFF_MULTICAST, "MULTICAST"),
  ] {
    if ife.flags.contains(bit) {
      flags.push(name);
    }
  }
  if flags.is_empty() {
    println!("          [NO FLAGS]");
  } else {
    println!("          {}", flags.join(" "));
  }
  println!();
}

fn display(only: Option<&str>, show_all: bool) -> i32 {
  let ifaces = match collect_interfaces(only) {
    Ok(v) => v,
    Err(e) => {
      eprintln!("ifconfig: {e}");
      return 1;
    }
  };
  if let Some(name) = only {
    if ifaces.is_empty() {
      eprintln!("ifconfig: {name}: error fetching interface information: Device not found");
      return 1;
    }
  }
  for ife in &ifaces {
    let up = ife.flags.contains(InterfaceFlags::IFF_UP);
    if only.is_some() || show_all || up {
      print_iface(ife);
    }
  }
  0
}

pub fn run(argv: &[&str]) -> i32 {
  let mut args = argv.iter().skip(1).copied().peekable();
  let mut show_all = false;
  if args.peek() == Some(&"-a") {
    show_all = true;
    args.next();
  }
  let rest: Vec<&str> = args.collect();

  match rest.len() {
    0 => display(None, show_all),
    1 => display(Some(rest[0]), show_all),
    _ => set_interface(rest[0], &rest[1..]),
  }
}

pub fn run_and_exit(args: &[&str]) -> ! {
  let code = run(args);
  std::process::exit(code);
}
