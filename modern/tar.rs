//! `tar` backed by the `tar` crate, composing with flate2/bzip2/xz2 for
//! `-z`/`-j`/`-J`. Common surface: `-c` create, `-x` extract, `-t` list;
//! `-f FILE` (else stdin/stdout), `-C DIR`, `-v` verbose. BusyBox-style bundled
//! or bare option clusters (`tar -czf a.tgz .` and `tar czf a.tgz .`).

use std::fs::File;
use std::io::{self, Read, Write};

#[derive(PartialEq)]
enum Mode {
  Create,
  Extract,
  List,
}

#[derive(Clone, Copy)]
enum Comp {
  None,
  Gz,
  Bz2,
  Xz,
}

pub fn run(argv: &[&str]) -> i32 {
  let mut mode: Option<Mode> = None;
  let mut comp = Comp::None;
  let mut verbose = false;
  let mut file: Option<String> = None;
  let mut chdir: Option<String> = None;
  let mut inputs: Vec<String> = Vec::new();

  let mut want: Option<char> = None; // pending arg for -f / -C
  let mut it = argv.iter().skip(1).copied().peekable();
  let mut first = true;
  while let Some(a) = it.next() {
    if let Some(w) = want.take() {
      match w {
        'f' => file = Some(a.to_string()),
        'C' => chdir = Some(a.to_string()),
        _ => {}
      }
      continue;
    }
    let is_cluster = a.starts_with('-')
      || (first && !a.is_empty() && a.chars().all(|c| "cxtzjJvftC".contains(c)));
    first = false;
    if is_cluster {
      for c in a.trim_start_matches('-').chars() {
        match c {
          'c' => mode = Some(Mode::Create),
          'x' => mode = Some(Mode::Extract),
          't' => mode = Some(Mode::List),
          'z' => comp = Comp::Gz,
          'j' => comp = Comp::Bz2,
          'J' => comp = Comp::Xz,
          'v' => verbose = true,
          'f' => want = Some('f'),
          'C' => want = Some('C'),
          _ => {}
        }
      }
    } else {
      inputs.push(a.to_string());
    }
  }

  let mode = match mode {
    Some(m) => m,
    None => {
      eprintln!("tar: need -c, -x or -t");
      return 2;
    }
  };

  let res = match mode {
    Mode::Create => create(&file, &chdir, &inputs, comp, verbose),
    Mode::Extract => extract(&file, &chdir, comp, verbose),
    Mode::List => list(&file, comp),
  };
  match res {
    Ok(()) => 0,
    Err(e) => {
      eprintln!("tar: {e}");
      1
    }
  }
}

fn wrap_writer(w: Box<dyn Write>, comp: Comp) -> io::Result<Box<dyn Write>> {
  Ok(match comp {
    Comp::None => w,
    #[cfg(feature = "modern-gzip")]
    Comp::Gz => Box::new(flate2::write::GzEncoder::new(w, flate2::Compression::new(6))),
    #[cfg(feature = "modern-bzip2")]
    Comp::Bz2 => Box::new(bzip2::write::BzEncoder::new(w, bzip2::Compression::new(9))),
    #[cfg(feature = "modern-xz")]
    Comp::Xz => Box::new(xz2::write::XzEncoder::new(w, 6)),
    #[allow(unreachable_patterns)]
    _ => return Err(unsupported()),
  })
}

fn wrap_reader(r: Box<dyn Read>, comp: Comp) -> io::Result<Box<dyn Read>> {
  Ok(match comp {
    Comp::None => r,
    #[cfg(feature = "modern-gzip")]
    Comp::Gz => Box::new(flate2::read::GzDecoder::new(r)),
    #[cfg(feature = "modern-bzip2")]
    Comp::Bz2 => Box::new(bzip2::read::BzDecoder::new(r)),
    #[cfg(feature = "modern-xz")]
    Comp::Xz => Box::new(xz2::read::XzDecoder::new(r)),
    #[allow(unreachable_patterns)]
    _ => return Err(unsupported()),
  })
}

fn unsupported() -> io::Error {
  io::Error::new(
    io::ErrorKind::Unsupported,
    "compression not built in (enable modern-gzip/bzip2/xz)",
  )
}

fn create(
  file: &Option<String>,
  chdir: &Option<String>,
  inputs: &[String],
  comp: Comp,
  verbose: bool,
) -> io::Result<()> {
  let sink: Box<dyn Write> = match file {
    Some(f) if f != "-" => Box::new(File::create(f)?),
    _ => Box::new(io::stdout()),
  };
  let mut builder = ::tar::Builder::new(wrap_writer(sink, comp)?);
  let base = chdir.as_deref().unwrap_or(".");
  for input in inputs {
    let path = std::path::Path::new(base).join(input);
    if verbose {
      eprintln!("{input}");
    }
    if path.is_dir() {
      builder.append_dir_all(input, &path)?;
    } else {
      builder.append_path_with_name(&path, input)?;
    }
  }
  builder.into_inner()?.flush()?;
  Ok(())
}

fn extract(
  file: &Option<String>,
  chdir: &Option<String>,
  comp: Comp,
  verbose: bool,
) -> io::Result<()> {
  let src: Box<dyn Read> = match file {
    Some(f) if f != "-" => Box::new(File::open(f)?),
    _ => Box::new(io::stdin()),
  };
  let mut ar = ::tar::Archive::new(wrap_reader(src, comp)?);
  let dest = chdir.as_deref().unwrap_or(".");
  for entry in ar.entries()? {
    let mut entry = entry?;
    if verbose {
      eprintln!("{}", entry.path()?.display());
    }
    entry.unpack_in(dest)?;
  }
  Ok(())
}

fn list(file: &Option<String>, comp: Comp) -> io::Result<()> {
  let src: Box<dyn Read> = match file {
    Some(f) if f != "-" => Box::new(File::open(f)?),
    _ => Box::new(io::stdin()),
  };
  let mut ar = ::tar::Archive::new(wrap_reader(src, comp)?);
  let stdout = io::stdout();
  let mut out = stdout.lock();
  for entry in ar.entries()? {
    let entry = entry?;
    writeln!(out, "{}", entry.path()?.display())?;
  }
  Ok(())
}
