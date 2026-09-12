#!/usr/bin/env bash
#
# The one command: build both native drivers, prove they were linked against
# the pinned core revisions, run them in a balanced order and write a report.
#
#   ./run.sh                      # quick profile (correctness first)
#   ./run.sh --profile standard   # the measurement profile
#   ./run.sh --profile both       # quick, then standard
#   ./run.sh --out ./my-results   # choose where the report goes
#
# This benchmark measures the two Amber cores' *library* operations in
# process, against each other, and nothing else: no CLI startup, no
# competitor backends.
#
# Nothing has to be prepared first. The Go driver depends on
# github.com/amber-store/core and the Rust driver on
# github.com/amber-store/core-rs, both pinned by core-ops/'s committed lock
# files to exactly the revisions this repository's flake.lock names. The
# script refuses to run if those pins ever disagree, and it records each
# core's revision, dirty state and each driver's SHA-256 in the report.
#
# Development overrides: set AMBER_GO_REPO and/or AMBER_RUST_REPO to a
# checkout of the corresponding core to measure that instead of the pinned
# revision. The report then records that checkout's revision and whether it
# was dirty, in place of the pin, and marks the run as not pin-verified.
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
here="$root/core-ops"

# Re-enter inside the pinned development shell exactly once.
if [ -z "${AMBER_CORE_OPS_IN_SHELL:-}" ]; then
  exec nix develop "$root" --command \
    env AMBER_CORE_OPS_IN_SHELL=1 bash "${BASH_SOURCE[0]}" "$@"
fi

profile=quick
out=""
scratch_parent=""
audit=""
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
    --scratch) scratch_parent=$2; shift 2 ;;
    --scratch=*) scratch_parent=${1#--scratch=}; shift ;;
    --audit) audit=$2; shift 2 ;;
    --audit=*) audit=${1#--audit=}; shift ;;
    --seed) seed=$2; shift 2 ;;
    --seed=*) seed=${1#--seed=}; shift ;;
    --cpus) cpus=$2; shift 2 ;;
    --cpus=*) cpus=${1#--cpus=}; shift ;;
    --only) only=$2; shift 2 ;;
    --only=*) only=${1#--only=}; shift ;;
    --keep-scratch) keep_scratch=yes; shift ;;
    -h|--help) sed -n '2,26p' "${BASH_SOURCE[0]}"; exit 0 ;;
    *) echo "run.sh: unknown argument $1" >&2; exit 2 ;;
  esac
done

case "$profile" in
  quick|standard|both) ;;
  *) echo "run.sh: --profile must be quick, standard or both" >&2; exit 2 ;;
esac

stamp=$(date -u +%Y%m%dT%H%M%SZ)
[ -n "$out" ] || out="$root/results/$stamp"
mkdir -p "$out"
out=$(cd "$out" && pwd)

# --------------------------------------------------------------------------
# Scratch: an owned, fresh child of the directory we were given
# --------------------------------------------------------------------------
#
# --scratch names a *parent*. This run creates one timestamped child under
# it, uses only that, and removes only that. A supplied directory that
# already existed is never removed, and two runs never share a scratch.
#
# Large scratch data belongs on a real filesystem, never on a tmpfs session
# directory: a standard run writes several gigabytes of segment files.
[ -n "$scratch_parent" ] || scratch_parent="${AMBER_CORE_OPS_SCRATCH:-/var/tmp/amber-core-ops}"
mkdir -p "$scratch_parent"
scratch_parent=$(cd "$scratch_parent" && pwd)

scratch_fs=$(df --output=fstype "$scratch_parent" | tail -1)
if [ "$scratch_fs" = "tmpfs" ]; then
  echo "run.sh: refusing to use a tmpfs scratch parent ($scratch_parent)." >&2
  echo "  Pass --scratch with a path on a real filesystem, e.g. /var/tmp/..." >&2
  exit 1
fi

scratch="$scratch_parent/run-$stamp-$$"
if [ -e "$scratch" ]; then
  echo "run.sh: scratch child $scratch already exists; refusing to reuse it" >&2
  exit 1
fi
mkdir "$scratch"                       # fails if the name is taken: we own it
scratch=$(cd "$scratch" && pwd)
owned_scratch="$scratch"               # the only path this script ever removes

cleanup() {
  if [ "$keep_scratch" = no ] && [ -n "${owned_scratch:-}" ] && [ -d "$owned_scratch" ]; then
    rm -rf -- "${owned_scratch:?}"
  fi
}
trap cleanup EXIT

# --------------------------------------------------------------------------
# Source identity, established before anything is built or measured
# --------------------------------------------------------------------------

lock="$root/flake.lock"
go_pin=$(jq -r '.nodes["amber-go-src"].locked.rev' "$lock")
rust_pin=$(jq -r '.nodes["amber-rust-src"].locked.rev' "$lock")
if [ "${#go_pin}" != 40 ] || [ "${#rust_pin}" != 40 ]; then
  echo "run.sh: flake.lock does not pin both core sources" >&2
  exit 1
fi

# The Rust driver names its core revision in its own manifest, and its lock
# file names it twice: once as the revision that was *asked* for and once, in
# the fragment after `#`, as the commit cargo actually resolved to. Both have
# to be the pin; the first without the second would let a moved branch or a
# stale registry entry pass as the pinned commit.
rust_dep_rev=$(sed -n 's/.*core-rs", rev = "\([0-9a-f]\{40\}\)".*/\1/p' "$here/rust/Cargo.toml")
rust_lock_req=$(sed -n 's|.*core-rs?rev=\([0-9a-f]\{40\}\)#.*|\1|p' "$here/rust/Cargo.lock" | head -1)
rust_lock_resolved=$(sed -n 's|.*core-rs?rev=[0-9a-f]\{40\}#\([0-9a-f]\{40\}\).*|\1|p' "$here/rust/Cargo.lock" | head -1)

pin_verified=true
note_mismatch() { pin_verified=false; echo "run.sh: $1" >&2; }

if [ -z "${AMBER_RUST_REPO:-}" ]; then
  for pair in "Cargo.toml dependency:$rust_dep_rev" \
              "Cargo.lock request:$rust_lock_req" \
              "Cargo.lock resolved commit:$rust_lock_resolved"; do
    what=${pair%%:*}; got=${pair##*:}
    if [ "$got" != "$rust_pin" ]; then
      echo "run.sh: rust/$what is $got, flake.lock pins $rust_pin" >&2
      exit 1
    fi
  done
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

# What cargo actually resolved, as opposed to what the lock file says it
# should have: `cargo metadata` reports the source of the dependency in the
# build that just happened.
rust_resolved_source=unknown
if [ "$rust_source" = pinned ]; then
  rust_resolved_source=$(cd "$rust_build_dir" && cargo metadata --format-version 1 --locked \
    | jq -r '.packages[] | select(.name == "amber-store-core") | .source' | head -1)
  case "$rust_resolved_source" in
    *"core-rs?rev=$rust_pin#$rust_pin") ;;
    *)
      echo "run.sh: cargo resolved amber-store-core from $rust_resolved_source," >&2
      echo "  which is not the pinned commit $rust_pin" >&2
      exit 1
      ;;
  esac
fi

# --------------------------------------------------------------------------
# Prove the Go driver really links the pinned source tree
# --------------------------------------------------------------------------
#
# go.sum pins the module's content hash, but not the commit it came from.
# The flake input is the commit. Comparing the resolved module against the
# flake's checkout closes that gap, so a report that claims a revision has
# actually measured it.
#
# The comparison covers *every* file, not only `.go`: assembly, cgo headers,
# embedded data, go.mod and go.sum all change what the driver links. The one
# legitimate difference is that a module zip carries no VCS metadata, so
# `.git` is excluded and nothing else is. Any other difference stops the run
# before a single measurement, rather than being explained away afterwards.
go_tree_verified=skipped
go_tree_diff=""
if [ "$go_source" = pinned ]; then
  pinned_src=$(nix build "$root#amber-go-src" --no-link --print-out-paths)
  module_dir=$(cd "$go_build_dir" && go list -m -f '{{.Dir}}' github.com/amber-store/core)
  go_tree_diff="$out/go-source-diff.txt"
  if diff -r -q -x '.git' "$pinned_src" "$module_dir" > "$go_tree_diff" 2>&1; then
    go_tree_verified=identical
    rm -f "$go_tree_diff"
    go_tree_diff=""
  else
    echo "run.sh: the resolved Go module differs from the pinned source tree." >&2
    echo "  Every build-relevant file must match; see $go_tree_diff" >&2
    sed -n '1,20p' "$go_tree_diff" >&2
    exit 1
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
  (
    cd "$root"
    {
      find core-ops -type f \( -name '*.go' -o -name '*.rs' -o -name '*.sh' -o -name '*.py' \
           -o -name 'go.mod' -o -name 'go.sum' -o -name 'Cargo.toml' -o -name 'Cargo.lock' \) \
        -not -path '*/target/*' -print0
      printf '%s\0' run.sh flake.nix flake.lock python/amber_bench_plot.py
    } | sort -z | xargs -0 sha256sum | sha256sum | cut -d' ' -f1
  )
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
  "schema": "amber-core-ops/identity/2",
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
      "cargo_lock_requested_rev": "$rust_lock_req",
      "cargo_lock_resolved_rev": "$rust_lock_resolved",
      "cargo_resolved_source": "$rust_resolved_source",
      "driver": "$rust_driver", "driver_sha256": "$rust_sha"
    }
  },
  "environment": {
    "host": "$(hostname)", "kernel": "$kernel", "cpu_model": "$cpu_model",
    "cpu_count": $(nproc), "cpu_set": "$cpu_list", "mem_total_kb": ${mem_total_kb:-0},
    "scratch": "$scratch", "scratch_fs": "$scratch_fs"
  }
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

# How many repetitions the profile asks for. run.sh splits them across two
# passes in opposite core order, so neither core is systematically measured
# on the colder or the busier machine; the report merges the passes and
# still requires exactly this many repetitions of every case.
profile_reps() {
  case "$1" in
    quick) echo 1 ;;
    standard) echo 7 ;;
  esac
}

drive() { # core, profile, out-file, scratch-suffix, rep-base, reps
  local core=$1 prof=$2 file=$3 suffix=$4 base=$5 reps=$6
  local driver args=(--profile "$prof" --seed "$seed" --scratch-fs "$scratch_fs"
                     --wire-dir "$scratch/wire-$prof" --rep-base "$base" --reps "$reps")
  if [ -n "$xattrs_flag" ]; then args+=("$xattrs_flag"); fi
  if [ -n "$only" ]; then args+=(--only "$only"); fi
  if [ "$core" = go ]; then
    driver=$go_driver
    args+=(--core-revision "$go_rev" --core-repo "$go_source")
    if [ "$go_dirty" = true ]; then args+=(--core-dirty); fi
  else
    driver=$rust_driver
    args+=(--core-revision "$rust_rev" --core-repo "$rust_source"
           --harness-revision "$harness_rev")
    if [ "$rust_dirty" = true ]; then args+=(--core-dirty); fi
    if [ "$harness_dirty" = yes ]; then args+=(--harness-dirty); fi
  fi
  taskset -c "$cpu_list" "$driver" "${args[@]}" \
    --out "$file" --scratch "$scratch/$suffix"
}

run_profile() {
  local prof=$1
  local pdir="$out/$prof"
  mkdir -p "$pdir"
  local reps; reps=$(profile_reps "$prof")

  # The wire packs, produced before anything is measured. Both cores encode
  # the same object population; their zstd encoders do not agree byte for
  # byte, so a decoder measured against "whatever this core wrote" would be
  # measured against a different input on each side. Both drivers read both
  # packs, and every reader and decoder case runs once per producer.
  mkdir -p "$scratch/wire-$prof"
  echo "==> $prof profile: producing both cores' wire packs"
  "$go_driver" --profile "$prof" --seed "$seed" --emit-wire --wire-dir "$scratch/wire-$prof"
  "$rust_driver" --profile "$prof" --seed "$seed" --emit-wire --wire-dir "$scratch/wire-$prof"

  # Two passes in opposite order, never overlapping. Pass one measures the
  # first half of the repetitions with Go first; pass two measures the rest
  # with Rust first.
  local first=$(( (reps + 1) / 2 ))
  local second=$(( reps - first ))

  echo "==> $prof profile: pass 1 (Go, then Rust), repetitions 0..$((first - 1))"
  drive go "$prof" "$pdir/go-pass1.json" "$prof-go-1" 0 "$first"
  drive rust "$prof" "$pdir/rust-pass1.json" "$prof-rust-1" 0 "$first"

  local go_docs=("$pdir/go-pass1.json") rust_docs=("$pdir/rust-pass1.json")
  if [ "$second" -gt 0 ]; then
    echo "==> $prof profile: pass 2 (Rust, then Go), repetitions $first..$((reps - 1))"
    drive rust "$prof" "$pdir/rust-pass2.json" "$prof-rust-2" "$first" "$second"
    drive go "$prof" "$pdir/go-pass2.json" "$prof-go-2" "$first" "$second"
    go_docs+=("$pdir/go-pass2.json")
    rust_docs+=("$pdir/rust-pass2.json")
  fi

  # ----------------------------------------------------------------------
  # Nothing was mutated while the run was in flight
  # ----------------------------------------------------------------------
  #
  # This happens *before* the report is written, not after it. A report that
  # looked valid and was then contradicted by a post-run check would be a
  # publishable artefact of an invalid run; there must be no window in which
  # one exists.
  verify_unmutated "$prof" "${go_docs[@]}" -- "${rust_docs[@]}"

  echo "==> $prof profile: report"
  local args=(--identity "$out/identity.json" --out "$pdir")
  local d
  for d in "${go_docs[@]}"; do args+=(--go "$d"); done
  for d in "${rust_docs[@]}"; do args+=(--rust "$d"); done
  python3 "$here/report/report.py" "${args[@]}"
}

# verify_unmutated re-checks every identity claim the report will rest on.
# Each raw document states which core revision, dirty state and executable
# hash produced it; those have to be the ones the identity document recorded
# before the run, and the executables and the benchmark's own sources have
# to be unchanged since.
verify_unmutated() {
  local prof=$1; shift
  local docs=() seen_sep=no d
  for d in "$@"; do
    if [ "$d" = "--" ]; then seen_sep=yes; continue; fi
    docs+=("$d")
  done
  : "$seen_sep"

  local sources_after; sources_after=$(sources_digest)
  if [ "$sources_before" != "$sources_after" ]; then
    echo "run.sh: the benchmark sources changed while the run was in flight" >&2
    echo "  before: $sources_before" >&2
    echo "  after:  $sources_after" >&2
    exit 1
  fi
  local go_after rust_after
  go_after=$(sha256sum "$go_driver" | cut -d' ' -f1)
  rust_after=$(sha256sum "$rust_driver" | cut -d' ' -f1)
  if [ "$go_sha" != "$go_after" ] || [ "$rust_sha" != "$rust_after" ]; then
    echo "run.sh: a driver executable changed while the run was in flight" >&2
    exit 1
  fi

  # Bind each raw document to the enclosing identity document.
  for d in "${docs[@]}"; do
    local core doc_rev doc_dirty doc_sha want_rev want_dirty want_sha
    core=$(jq -r '.core' "$d")
    doc_rev=$(jq -r '.identity.core_revision' "$d")
    doc_dirty=$(jq -r '.identity.core_dirty' "$d")
    doc_sha=$(jq -r '.identity.driver_sha256' "$d")
    want_rev=$(jq -r --arg c "$core" '.cores[$c].revision' "$out/identity.json")
    want_dirty=$(jq -r --arg c "$core" '.cores[$c].dirty' "$out/identity.json")
    want_sha=$(jq -r --arg c "$core" '.cores[$c].driver_sha256' "$out/identity.json")
    if [ "$doc_rev" != "$want_rev" ] || [ "$doc_dirty" != "$want_dirty" ] \
       || [ "$doc_sha" != "$want_sha" ]; then
      echo "run.sh: $d does not match the recorded identity:" >&2
      echo "  document: rev=$doc_rev dirty=$doc_dirty sha=$doc_sha" >&2
      echo "  identity: rev=$want_rev dirty=$want_dirty sha=$want_sha" >&2
      exit 1
    fi
  done
  echo "==> $prof profile: every raw document is bound to the recorded identity"
}

# The two drivers run one after the other, never together: overlapping them
# would measure contention rather than the operations.
case "$profile" in
  quick)    run_profile quick ;;
  standard) run_profile standard ;;
  both)     run_profile quick; run_profile standard ;;
esac

# --------------------------------------------------------------------------
# Audit copy
# --------------------------------------------------------------------------
#
# The essential raw artefacts, copied where an audit can find them without
# the results directory: the identity document and every raw sample document.
if [ -n "$audit" ]; then
  mkdir -p "$audit/$stamp"
  cp "$out/identity.json" "$audit/$stamp/"
  for p in quick standard; do
    if [ -d "$out/$p" ]; then
      mkdir -p "$audit/$stamp/$p"
      cp "$out/$p"/*.json "$out/$p"/REPORT.md "$audit/$stamp/$p/" 2>/dev/null || true
    fi
  done
  echo "==> copied the raw reports into $audit/$stamp"
fi

echo "==> done: $out"
