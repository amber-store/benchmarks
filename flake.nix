{
  description =
    "Native operation-by-operation benchmark of the two Amber-Store cores";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
    systems.url = "github:nix-systems/default";

    # The two cores under comparison, pinned by revision. This repository is
    # standalone: `./run.sh` builds both native drivers against these inputs
    # and needs no sibling checkout of either core. Overriding one with a
    # local checkout is a development convenience, not the default -- see
    # AMBER_GO_REPO and AMBER_RUST_REPO in run.sh, which record that
    # checkout's revision and dirty state in the report instead of the pin
    # below, and mark the run as not pin-verified.
    amber-rust-src = {
      url =
        "github:amber-store/core-rs/141df2b0a9a8ad1726769f63811db3a166be188a";
      flake = false;
    };
    amber-go-src = {
      url = "github:amber-store/core/4ed4660657b12421a534ab0b08cfd717ae3d2291";
      flake = false;
    };
  };

  outputs = { self, nixpkgs, systems, amber-rust-src, amber-go-src, ... }:
    let
      eachSystem = f:
        nixpkgs.lib.genAttrs (import systems)
        (system: f system nixpkgs.legacyPackages.${system});

      # Revisions of the pinned core sources, recorded in every report.
      rustRev = amber-rust-src.rev;
      goRev = amber-go-src.rev;

      # Matplotlib, for the chart renderer in python/. The committed Markdown
      # and SVG are readable without any of it.
      pythonEnv = pkgs: pkgs.python3.withPackages (ps: [ ps.matplotlib ]);
    in {
      packages = eachSystem (system: pkgs: {
        # The pinned core sources themselves, as buildable paths.
        #
        # `run.sh` compares the Go module its driver resolved against
        # `amber-go-src`, because a module hash pins content while the flake
        # input pins the commit: a report that names a revision should have
        # measured that revision. The Rust side needs no such step, because
        # Cargo.lock names the commit directly -- but the same path is used
        # to re-derive the Rust source for the Cargo.lock cross-check.
        amber-go-src =
          pkgs.runCommandLocal "amber-go-src-${builtins.substring 0 12 goRev}"
          { } ''
            cp -r ${amber-go-src} "$out"
          '';
        amber-rust-src = pkgs.runCommandLocal
          "amber-rust-src-${builtins.substring 0 12 rustRev}" { } ''
            cp -r ${amber-rust-src} "$out"
          '';
      });

      devShells = eachSystem (system: pkgs: {
        default = pkgs.mkShell {
          hardeningDisable = [ "all" ];
          packages = with pkgs; [
            # the Rust driver
            cargo
            rustc
            rustfmt
            clippy
            # the Go driver and the Go export extractor
            go
            # pinning both native drivers to one CPU set (taskset) and
            # probing the scratch filesystem for extended attributes
            # (setfattr) before either driver builds a fixture
            util-linux
            attr
            # the report, its tests and the chart renderer
            (pythonEnv pkgs)
            # helpers used by run.sh
            jq
            coreutils
            git
          ];
        };
      });

      # `nix fmt` formats everything this repository owns: the Rust driver
      # (its own crate) and both Go programs.
      formatter = eachSystem (_system: pkgs:
        pkgs.writeShellScriptBin "fmt" ''
          set -euo pipefail
          if [ -f core-ops/rust/Cargo.toml ]; then
            (cd core-ops/rust && ${pkgs.rustfmt}/bin/cargo-fmt --all)
          fi
          for dir in core-ops/go core-ops/coverage/goexports; do
            if [ -d "$dir" ]; then
              ${pkgs.go}/bin/gofmt -w "$dir"
            fi
          done
        '');
    };
}
