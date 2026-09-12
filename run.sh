#!/usr/bin/env bash
#
# The one command: build the pinned tools, build both Amber cores, run the
# whole benchmark suite and write a report.
#
#   ./run.sh                       # smoke profile, all backends
#   ./run.sh --profile standard    # bigger
#   ./run.sh --out ./my-results    # choose where the report goes
#
# Everything after the script name is passed to `amber-cas-bench run`, so any
# flag that command takes works here too (--groups, --backends, --seed,
# --repeats, --jobs, --blob-latency-ms, --remote-s3-endpoint, ...).
#
# Nothing has to be prepared first, and no checkout of either core has to
# exist next to this one. The external tools (git, restic, nix, garage, mc)
# come from this directory's flake, and *both* cores are built from the
# source revisions pinned as the flake's `amber-rust-src` and `amber-go-src`
# inputs. The harness records each executable's path, version, SHA-256 and
# source revision in the report.
#
# Development overrides: set AMBER_RUST_REPO and/or AMBER_GO_REPO to a
# checkout of the corresponding core to measure that instead of the pinned
# revision. The report then records that checkout's revision and whether it
# was dirty, in place of the pin.
set -euo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)

# Re-enter inside the pinned development shell exactly once. Everything below
# this line runs with the flake's toolchain, git, restic, nix, garage and mc.
if [ -z "${AMBER_BENCH_IN_SHELL:-}" ]; then
  exec nix develop "$here" --command \
    env AMBER_BENCH_IN_SHELL=1 bash "${BASH_SOURCE[0]}" "$@"
fi

out=""
args=()
while [ $# -gt 0 ]; do
  case "$1" in
    --out) out=$2; args+=("$1" "$2"); shift 2 ;;
    --out=*) out=${1#--out=}; args+=("$1"); shift ;;
    *) args+=("$1"); shift ;;
  esac
done
if [ -z "$out" ]; then
  out="$PWD/benchmark-results/$(date -u +%Y%m%dT%H%M%SZ)"
  args+=(--out "$out")
fi

# Builds one core, either from its pinned flake input or -- if the matching
# *_REPO variable names a checkout -- from that checkout. Appends the
# resulting --amber-<core>-bin and source-identity flags to core_args.
core_args=()
build_core() {
  local core=$1 repo=$2 build=$3

  if [ -n "$repo" ]; then
    repo=$(cd "$repo" && pwd)
    local rev dirty=no
    rev=$(git -C "$repo" rev-parse HEAD 2>/dev/null || echo unknown)
    [ -n "$(git -C "$repo" status --porcelain 2>/dev/null || true)" ] && dirty=yes
    echo "==> building the $core core's amber-store CLI from $repo"
    echo "    local checkout override: revision $rev, dirty: $dirty"
    mkdir -p "$here/target/cores"
    "$build" "$repo" "$here/target/cores/$core-amber-store"
    core_args+=("--amber-$core-bin" "$here/target/cores/$core-amber-store"
                "--amber-$core-repo" "$repo")
  else
    echo "==> building the $core core's amber-store CLI from the pinned flake input"
    local store rev
    store=$(nix build "$here#amber-$core" --no-link --print-out-paths)
    rev=$(nix eval --raw "$here#amber-$core.sourceRevision")
    echo "    pinned revision $rev"
    core_args+=("--amber-$core-bin" "$store/bin/amber-store"
                "--amber-$core-rev" "$rev")
  fi
}

build_rust_checkout() {
  local repo=$1 dest=$2
  if [ ! -f "$repo/examples/amber-store.rs" ]; then
    echo "run.sh: AMBER_RUST_REPO=$repo has no examples/amber-store.rs" >&2
    exit 1
  fi
  cargo build --release --locked --manifest-path "$repo/Cargo.toml" \
    --example amber-store
  install -m755 "$repo/target/release/examples/amber-store" "$dest"
}

build_go_checkout() {
  local repo=$1 dest=$2
  if [ ! -d "$repo/cmd/amber-store" ]; then
    echo "run.sh: AMBER_GO_REPO=$repo has no cmd/amber-store" >&2
    exit 1
  fi
  (cd "$repo" && go build -o "$dest" ./cmd/amber-store)
}

build_core rust "${AMBER_RUST_REPO:-}" build_rust_checkout
build_core go "${AMBER_GO_REPO:-}" build_go_checkout

echo "==> building the benchmark harness (locked)"
cargo build --release --locked --manifest-path "$here/Cargo.toml"

echo "==> running the suite; the report will be written to $out"
exec "$here/target/release/amber-cas-bench" run \
  --harness-repo "$here" \
  --flake-dir "$here" \
  "${core_args[@]}" \
  "${args[@]}"
