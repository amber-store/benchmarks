#!/usr/bin/env bash
#
# Re-extract both cores' exported surfaces from the pinned sources and
# regenerate ../COVERAGE.md from the matrix. Run this whenever flake.lock
# moves either core: the check will then say exactly which new symbols are
# unaccounted for.
set -euo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/../.." && pwd)

if [ -z "${AMBER_CORE_OPS_IN_SHELL:-}" ]; then
  exec nix develop "$root" --command \
    env AMBER_CORE_OPS_IN_SHELL=1 bash "${BASH_SOURCE[0]}" "$@"
fi

go_src=$(nix build "$root#amber-go-src" --no-link --print-out-paths)
rust_src=$(nix build "$root#amber-rust-src" --no-link --print-out-paths)

echo "==> extracting the Go core's exported API from $go_src"
(cd "$here/goexports" && go run . "$go_src") > "$here/go-exports.txt"

echo "==> extracting the Rust core's exported API from $rust_src"
python3 "$here/rustexports.py" "$rust_src" > "$here/rust-exports.txt"

echo "==> checking and rendering the matrix"
python3 "$here/matrix.py" render "$@"
