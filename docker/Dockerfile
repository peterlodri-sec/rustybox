# Linux build environment for rustybox.
# rustybox is Linux-only (raw syscalls, glibc/musl headers) so all builds and
# tests run in here — works identically on macOS/Windows hosts via Docker.
FROM debian:bookworm-slim

ENV DEBIAN_FRONTEND=noninteractive \
    RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH

RUN apt-get update && apt-get install -y --no-install-recommends \
      ca-certificates curl build-essential pkg-config \
      musl-tools musl-dev clang lld git \
    && rm -rf /var/lib/apt/lists/*

# rustup + the exact toolchain pinned by rust-toolchain.toml is installed on
# first `cargo` invocation via the mounted workspace. Bootstrap rustup here.
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
      | sh -s -- -y --default-toolchain none --profile minimal

# Wild linker for fast incremental links (David Lattimore's linker).
# Built once into the image; opt-in at build time (see build.sh).
RUN /usr/local/cargo/bin/rustup toolchain install stable --profile minimal \
    && /usr/local/cargo/bin/cargo +stable install --locked wild-linker \
    && rustup toolchain uninstall stable

WORKDIR /work
