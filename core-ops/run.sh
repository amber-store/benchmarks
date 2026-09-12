#!/usr/bin/env bash
#
# The one command for the core-operation benchmark: build both native
# drivers, prove they were linked against the pinned core revisions, run them
# sequentially and write a report.
#
#   ./core-ops/run.sh                      # quick profile (correctness first)
#   ./core-ops/run.sh --profile standard   # the measurement profile
#   ./core-ops/run.sh --profile both       # quick, then standard
#   ./core-ops/run.sh --out ./my-results   # choose where the report goes
#
# This is a different benchmark from the cross-system CAS suite in ./run.sh.
# It measures the two cores' *library* operations in process, against each
# other, and nothing else: no CLI startup, no competitor backends. Its
# results live under core-ops-results/ and never mix with results/.
#
# Nothing has to be prepared first. The Go driver depends on
# github.com/amber-store/core and the Rust driver on
# github.com/amber-store/core-rs, both pinned by this directory's committed
# lock files to exactly the revisions the repository's flake.lock names. The
# script refuses to run if those pins ever disagree, and it records each
# core's revision, dirty state and each driver's SHA-256 in the report.
#
# Development overrides: set AMBER_GO_REPO and/or AMBER_RUST_REPO to a
# checkout of the corresponding core to measure that instead of the pinned
# revision. The report then records that checkout's revision and whether it
# was dirty, in place of the pin, and marks the run as not pin-verified.
set -euo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)

# Re-enter inside the pinned development shell exactly once.
if [ -z "${AMBER_CORE_OPS_IN_SHELL:-}" ]; then
  exec nix develop "$root" --command \
    env AMBER_CORE_OPS_IN_SHELL=1 bash "${BASH_SOURCE[0]}" "$@"
fi

profile=quick
out=""
scratch=""
seed=1592639710
cpus=8
only=""
keep_scratch=no

while [ $# -gt 0 ]; do
  case "$1" in
    --profile) profile=$2; shift 2 ;;
    --profile=*) profile=${1#--profile=}; shift ;;
    --out) out=$2; shift 2 ;;
    --out=*) out=${1#--out=}; shift ;;
    --scratch) scratch=$2; shift 2 ;;
    --scratch=*) scratch=${1#--scratch=}; shift ;;
    --seed) seed=$2; shift 2 ;;
    --seed=*) seed=${1#--seed=}; shift ;;
    --cpus) cpus=$2; shift 2 ;;
    --cpus=*) cpus=${1#--cpus=}; shift ;;
    --only) only=$2; shift 2 ;;
    --only=*) only=${1#--only=}; shift ;;
    --keep-scratch) keep_scratch=yes; shift ;;
    -h|--help) sed -n '2,28p' "${BASH_SOURCE[0]}"; exit 0 ;;
    *) echo "core-ops/run.sh: unknown argument $1" >&2; exit 2 ;;
  esac
done

case "$profile" in
  quick|standard|both) ;;
  *) echo "core-ops/run.sh: --profile must be quick, standard or both" >&2; exit 2 ;;
esac

stamp=$(date -u +%Y%m%dT%H%M%SZ)
[ -n "$out" ] || out="$root/core-ops-results/$stamp"
mkdir -p "$out"
out=$(cd "$out" && pwd)

# Large scratch data belongs on a real filesystem, never on a tmpfs session
# directory: a standard run writes several gigabytes of segment files. The
# directory is timestamped, so no run ever reuses another's scratch.
[ -n "$scratch" ] || scratch="${AMBER_CORE_OPS_SCRATCH:-/var/tmp/amber-core-ops}/$stamp"
mkdir -p "$scratch"
scratch=$(cd "$scratch" && pwd)
scratch_fs=$(df --output=fstype "$scratch" | tail -1)
if [ "$scratch_fs" = "tmpfs" ]; then
  echo "core-ops/run.sh: refusing to use a tmpfs scratch directory ($scratch)." >&2
  echo "  Pass --scratch with a path on a real filesystem, e.g. /var/tmp/..." >&2
  exit 1
fi

# --------------------------------------------------------------------------
# Source identity, established before anything is built or measured
# --------------------------------------------------------------------------

lock="$root/flake.lock"
go_pin=$(jq -r '.nodes["amber-go-src"].locked.rev' "$lock")
rust_pin=$(jq -r '.nodes["amber-rust-src"].locked.rev' "$lock")
if [ "${#go_pin}" != 40 ] || [ "${#rust_pin}" != 40 ]; then
  echo "core-ops/run.sh: flake.lock does not pin both core sources" >&2
  exit 1
fi

# The Rust driver names its core revision in its own manifest and lock file.
rust_dep_rev=$(sed -n 's/.*core-rs", rev = "\([0-9a-f]\{40\}\)".*/\1/p' "$here/rust/Cargo.toml")
rust_lock_rev=$(sed -n 's|.*core-rs?rev=\([0-9a-f]\{40\}\)#.*|\1|p' "$here/rust/Cargo.lock" | head -1)

pin_verified=true
note_mismatch() { pin_verified=false; echo "core-ops/run.sh: $1" >&2; }

if [ -z "${AMBER_RUST_REPO:-}" ]; then
  if [ "$rust_dep_rev" != "$rust_pin" ]; then
    echo "core-ops/run.sh: rust/Cargo.toml pins $rust_dep_rev, flake.lock pins $rust_pin" >&2
    exit 1
  fi
  if [ "$rust_lock_rev" != "$rust_pin" ]; then
    echo "core-ops/run.sh: rust/Cargo.lock pins $rust_lock_rev, flake.lock pins $rust_pin" >&2
    exit 1
  fi
fi

echo "==> pinned core revisions"
echo "    go:   $go_pin"
echo "    rust: $rust_pin"

harness_rev=$(git -C "$root" rev-parse HEAD 2>/dev/null || echo unknown)
harness_dirty=no
if [ -n "$(git -C "$root" status --porcelain 2>/dev/null || true)" ]; then
  harness_dirty=yes
fi

# --------------------------------------------------------------------------
# Build the drivers
# --------------------------------------------------------------------------

build_dir="$root/target/core-ops"
mkdir -p "$build_dir"

go_source=pinned
go_rev="$go_pin"
go_dirty=false
go_driver="$build_dir/amber-core-ops-go"
go_build_dir="$here/go"

if [ -n "${AMBER_GO_REPO:-}" ]; then
  repo=$(cd "$AMBER_GO_REPO" && pwd)
  go_source=local
  go_rev=$(git -C "$repo" rev-parse HEAD 2>/dev/null || echo unknown)
  if [ -n "$(git -C "$repo" status --porcelain 2>/dev/null || true)" ]; then
    go_dirty=true
  fi
  note_mismatch "AMBER_GO_REPO overrides the Go pin with $repo ($go_rev, dirty=$go_dirty)"
  go_build_dir="$build_dir/go-override"
  rm -rf -- "${build_dir:?}/go-override"
  cp -r "$here/go" "$go_build_dir"
  chmod -R u+w "$go_build_dir"
  (cd "$go_build_dir" && go mod edit -replace "github.com/amber-store/core=$repo" && go mod tidy >/dev/null)
fi

echo "==> building the Go driver ($go_source)"
(cd "$go_build_dir" && CGO_ENABLED=0 go build -trimpath -o "$go_driver" .)

rust_source=pinned
rust_rev="$rust_pin"
rust_dirty=false
rust_build_dir="$here/rust"

if [ -n "${AMBER_RUST_REPO:-}" ]; then
  repo=$(cd "$AMBER_RUST_REPO" && pwd)
  rust_source=local
  rust_rev=$(git -C "$repo" rev-parse HEAD 2>/dev/null || echo unknown)
  if [ -n "$(git -C "$repo" status --porcelain 2>/dev/null || true)" ]; then
    rust_dirty=true
  fi
  note_mismatch "AMBER_RUST_REPO overrides the Rust pin with $repo ($rust_rev, dirty=$rust_dirty)"
  rust_build_dir="$build_dir/rust-override"
  rm -rf -- "${build_dir:?}/rust-override"
  cp -r "$here/rust" "$rust_build_dir"
  chmod -R u+w "$rust_build_dir"
  cat >> "$rust_build_dir/Cargo.toml" <<EOF

[patch."https://github.com/amber-store/core-rs"]
amber-store-core = { path = "$repo" }
EOF
fi

echo "==> building the Rust driver ($rust_source)"
if [ "$rust_source" = pinned ]; then
  (cd "$rust_build_dir" && cargo build --release --locked)
else
  (cd "$rust_build_dir" && cargo build --release)
fi
rust_driver="$build_dir/amber-core-ops-rs"
install -m755 "$rust_build_dir/target/release/amber-core-ops-rs" "$rust_driver"

# --------------------------------------------------------------------------
# Prove the Go driver really links the pinned source tree
# --------------------------------------------------------------------------
#
# go.sum pins the module's content hash, but not the commit it came from.
# The flake input is the commit. Comparing every Go source file of the
# resolved module against the flake's checkout closes that gap, so a report
# that claims a revision has actually measured it. (The Rust side needs no
# such step: Cargo.lock names the commit directly.)
go_tree_verified=skipped
if [ "$go_source" = pinned ]; then
  pinned_src=$(nix build "$root#amber-go-src" --no-link --print-out-paths)
  module_dir=$(cd "$go_build_dir" && go list -m -f '{{.Dir}}' github.com/amber-store/core)
  if diff -r -q -x '.git' "$pinned_src" "$module_dir" > "$out/go-source-diff.txt" 2>&1; then
    go_tree_verified=identical
    rm -f "$out/go-source-diff.txt"
  elif grep -qE '\.go( |$)|\.go differ' "$out/go-source-diff.txt"; then
    go_tree_verified=differs
    note_mismatch "the resolved Go module differs from the pinned source tree; see $out/go-source-diff.txt"
  else
    # A module zip legitimately omits some non-Go files; only a difference in
    # code would invalidate the identity claim.
    go_tree_verified=identical-code
  fi
fi
echo "    go module tree vs pinned source: $go_tree_verified"

# --------------------------------------------------------------------------
# Executable and source identity, recorded before a single measurement
# --------------------------------------------------------------------------

go_sha=$(sha256sum "$go_driver" | cut -d' ' -f1)
rust_sha=$(sha256sum "$rust_driver" | cut -d' ' -f1)
# A digest of the benchmark's own sources, taken before and after the run, so
# a report can state that nothing was edited while it was being measured.
sources_digest() {
  find "$here" -type f \( -name '*.go' -o -name '*.rs' -o -name '*.sh' -o -name '*.py' \
       -o -name 'go.mod' -o -name 'go.sum' -o -name 'Cargo.toml' -o -name 'Cargo.lock' \) \
    -not -path '*/target/*' -print0 | sort -z | xargs -0 sha256sum | sha256sum | cut -d' ' -f1
}
sources_before=$(sources_digest)

cpu_list="0-$((cpus - 1))"
kernel=$(uname -sr)
cpu_model=$(sed -n 's/^model name[ \t]*: //p' /proc/cpuinfo | head -1)
mem_total_kb=$(sed -n 's/^MemTotal:[ \t]*\([0-9]*\) kB/\1/p' /proc/meminfo)
go_dirty_json=$go_dirty
rust_dirty_json=$rust_dirty
harness_dirty_json=false
if [ "$harness_dirty" = yes ]; then harness_dirty_json=true; fi

cat > "$out/identity.json" <<EOF
{
  "schema": "amber-core-ops/identity/1",
  "recorded_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "pin_verified": $pin_verified,
  "flake_lock": { "amber_go_src": "$go_pin", "amber_rust_src": "$rust_pin" },
  "harness": {
    "revision": "$harness_rev",
    "dirty": $harness_dirty_json,
    "sources_sha256": "$sources_before"
  },
  "cores": {
    "go": {
      "source": "$go_source", "revision": "$go_rev", "dirty": $go_dirty_json,
      "module": "github.com/amber-store/core",
      "module_tree_vs_pinned_source": "$go_tree_verified",
      "driver": "$go_driver", "driver_sha256": "$go_sha"
    },
    "rust": {
      "source": "$rust_source", "revision": "$rust_rev", "dirty": $rust_dirty_json,
      "module": "amber-store-core",
      "cargo_lock_rev": "$rust_lock_rev",
      "driver": "$rust_driver", "driver_sha256": "$rust_sha"
    }
  },
  "environment": {
    "host": "$(hostname)", "kernel": "$kernel", "cpu_model": "$cpu_model",
    "cpu_count": $(nproc), "cpu_set": "$cpu_list", "mem_total_kb": ${mem_total_kb:-0},
    "scratch": "$scratch", "scratch_fs": "$scratch_fs"
  },
  "profiles": []
}
EOF
echo "==> recorded source and executable identity in $out/identity.json"

# Does the scratch filesystem carry user extended attributes? Both drivers
# are told the same answer, so neither builds a fixture the other cannot.
xattr_probe="$scratch/xattr-probe"
: > "$xattr_probe"
xattrs_flag=""
if setfattr -n user.amber.probe -v 1 "$xattr_probe" 2>/dev/null; then
  xattrs_flag="--xattrs"
fi
rm -f "$xattr_probe"

# --------------------------------------------------------------------------
# Measure
# --------------------------------------------------------------------------

run_profile() {
  local prof=$1
  local pdir="$out/$prof"
  mkdir -p "$pdir"
  local args=(--profile "$prof" --seed "$seed" --scratch-fs "$scratch_fs")
  if [ -n "$xattrs_flag" ]; then args+=("$xattrs_flag"); fi
  if [ -n "$only" ]; then args+=(--only "$only"); fi

  local go_extra=() rust_extra=()
  if [ "$go_dirty" = true ]; then go_extra+=(--core-dirty); fi
  if [ "$rust_dirty" = true ]; then rust_extra+=(--core-dirty); fi
  if [ "$harness_dirty" = yes ]; then rust_extra+=(--harness-dirty); fi

  echo "==> $prof profile: Go driver"
  taskset -c "$cpu_list" "$go_driver" "${args[@]}" \
    --out "$pdir/go.json" --scratch "$scratch/$prof-go" \
    --core-revision "$go_rev" --core-repo "$go_source" "${go_extra[@]}"

  echo "==> $prof profile: Rust driver"
  taskset -c "$cpu_list" "$rust_driver" "${args[@]}" \
    --out "$pdir/rust.json" --scratch "$scratch/$prof-rust" \
    --core-revision "$rust_rev" --core-repo "$rust_source" \
    --harness-revision "$harness_rev" "${rust_extra[@]}"

  echo "==> $prof profile: report"
  python3 "$here/report/report.py" \
    --identity "$out/identity.json" \
    --go "$pdir/go.json" --rust "$pdir/rust.json" \
    --out "$pdir"
}

# The two drivers run one after the other, never together: overlapping them
# would measure contention rather than the operations.
case "$profile" in
  quick)    run_profile quick ;;
  standard) run_profile standard ;;
  both)     run_profile quick; run_profile standard ;;
esac

sources_after=$(sources_digest)
if [ "$sources_before" != "$sources_after" ]; then
  echo "core-ops/run.sh: the benchmark sources changed while the run was in flight" >&2
  echo "  before: $sources_before" >&2
  echo "  after:  $sources_after" >&2
  exit 1
fi
go_sha_after=$(sha256sum "$go_driver" | cut -d' ' -f1)
rust_sha_after=$(sha256sum "$rust_driver" | cut -d' ' -f1)
if [ "$go_sha" != "$go_sha_after" ] || [ "$rust_sha" != "$rust_sha_after" ]; then
  echo "core-ops/run.sh: a driver executable changed while the run was in flight" >&2
  exit 1
fi

if [ "$keep_scratch" = no ]; then
  rm -rf -- "${scratch:?}"
fi

echo "==> done: $out"
