#!/usr/bin/env bash
# Build/test rustybox inside the Linux build container.
#
#   docker/build.sh                 # build curated-core feature set
#   docker/build.sh --all-features  # build everything
#   docker/build.sh test            # run the test suite
#   RUSTYBOX_WILD=1 docker/build.sh # link with the wild linker
#
# The workspace and cargo registry/target caches are bind-mounted so rebuilds
# are incremental across runs.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
IMAGE="rustybox-build:latest"

# Curated-core applets (see README "curated core"). Everything still *compiles*
# via --all-features; this is the default deep-verified set.
CORE_FEATURES="cat ls echo cp mv rm mkdir rmdir ln pwd touch true false \
  head tail wc sort uniq grep sed cut tr chmod chown df du ps kill sleep \
  env printenv date basename dirname readlink stat which id whoami ash"

docker build -t "$IMAGE" -f "$REPO_ROOT/docker/Dockerfile" "$REPO_ROOT"

WILD_ENV=()
if [[ "${RUSTYBOX_WILD:-0}" == "1" ]]; then
  # Route the linker through clang + wild for both musl targets.
  WILD_ENV=(-e CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS="-C target-feature=+crt-static -C linker=clang -C link-arg=--ld-path=wild"
            -e CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_RUSTFLAGS="-C target-feature=+crt-static -C linker=clang -C link-arg=--ld-path=wild")
fi

CMD="${1:-build}"
shift || true

case "$CMD" in
  build)
    RUN=(cargo build --release --no-default-features --features "$CORE_FEATURES") ;;
  --all-features)
    RUN=(cargo build --release --all-features) ;;
  test)
    RUN=(cargo test --no-default-features --features "$CORE_FEATURES" "$@") ;;
  check)
    RUN=(cargo build --all-features "$@") ;;
  *)
    RUN=(cargo "$CMD" "$@") ;;
esac

exec docker run --rm -it \
  -v "$REPO_ROOT":/work \
  -v rustybox-cargo-registry:/usr/local/cargo/registry \
  -v rustybox-target:/work/target \
  "${WILD_ENV[@]}" \
  "$IMAGE" \
  "${RUN[@]}"
