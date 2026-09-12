{
  description = "Reproducible CAS comparison benchmarks for the Amber-Store cores";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
    systems.url = "github:nix-systems/default";

    # The two cores under comparison, pinned by revision. This repository is
    # standalone: the default one-command path builds both binaries from
    # these inputs and needs no sibling checkout of either core. Overriding
    # one with a local checkout is a development convenience, not the
    # default -- see AMBER_RUST_REPO and AMBER_GO_REPO in run.sh, which
    # record that checkout's revision and dirty state in the report instead
    # of the pin below.
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

  outputs =
    { self, nixpkgs, systems, amber-rust-src, amber-go-src, ... }:
    let
      eachSystem = f:
        nixpkgs.lib.genAttrs (import systems)
        (system: f system nixpkgs.legacyPackages.${system});

      # Revisions of the pinned core sources, recorded in every report
      # produced by the default path.
      rustRev = amber-rust-src.rev;
      goRev = amber-go-src.rev;

      # Everything the notebook needs. Pinned by this flake's lock file like
      # every other tool, so `results/*/explore.ipynb` runs the same way on
      # another machine. The committed Markdown and SVG are readable without
      # any of it.
      pythonEnv = pkgs:
        pkgs.python3.withPackages (ps: [
          ps.jupyter-core
          ps.jupyterlab
          ps.nbclient
          ps.nbconvert
          ps.nbformat
          ps.ipykernel
          ps.pandas
          ps.matplotlib
          ps.tabulate
        ]);
    in {
      # Every external tool the harness shells out to is pinned by this
      # flake's lock file, so a rerun on another machine measures the same
      # binaries. The harness additionally records each tool's reported
      # version and the SHA-256 of its executable in the report.
      packages = eachSystem (system: pkgs:
        let fixtures = pkgs.callPackage ./nix/fixtures.nix { };
        in {
          # Deterministic Nix closures used by the "nix" scenario group.
          nix-fixtures-smoke = fixtures.smoke;
          nix-fixtures-standard = fixtures.standard;
          nix-fixtures-large = fixtures.large;

          # The Rust core's command line, built from the pinned source
          # above. `amber-store` is an example of that crate rather than a
          # bin target, so it is built and installed explicitly.
          amber-rust = pkgs.rustPlatform.buildRustPackage {
            pname = "amber-store-rust";
            version = "0.2.0-${builtins.substring 0 12 rustRev}";
            src = amber-rust-src;
            cargoLock.lockFile = "${amber-rust-src}/Cargo.lock";
            cargoBuildFlags = [ "--example" "amber-store" ];
            # The benchmark only needs the executable; the core's own test
            # suite is run by its own CI.
            doCheck = false;
            installPhase = ''
              runHook preInstall
              mkdir -p "$out/bin"
              found=$(find target -type f -path '*release/examples/amber-store' \
                        -perm -u+x -print -quit)
              test -n "$found" || { echo "amber-store example was not built"; exit 1; }
              install -m555 "$found" "$out/bin/amber-store"
              runHook postInstall
            '';
            passthru.sourceRevision = rustRev;
          };

          # The Go core's command line, built from the pinned source above.
          amber-go = pkgs.buildGoModule {
            pname = "amber-store-go";
            version = "0.0.7-${builtins.substring 0 12 goRev}";
            src = amber-go-src;
            vendorHash = "sha256-KXsK1sIsUndwl8fauX/DibW0gZqrSpMGqurSbpLKTwY=";
            subPackages = [ "cmd/amber-store" ];
            doCheck = false;
            passthru.sourceRevision = goRev;
          };

          # The harness itself, so `nix build .#amber-cas-bench` gives a
          # runnable command line without a Rust toolchain in $PATH.
          amber-cas-bench = pkgs.rustPlatform.buildRustPackage {
            pname = "amber-cas-bench";
            version = "0.1.0";
            src = self;
            cargoLock.lockFile = ./Cargo.lock;
          };

          # The pinned notebook environment, also usable on its own:
          #   nix run .#jupyter -- lab
          jupyter = pythonEnv pkgs;
        });

      devShells = eachSystem (system: pkgs: {
        default = pkgs.mkShell {
          hardeningDisable = [ "all" ];
          packages = with pkgs; [
            # this harness
            cargo
            rustc
            rustfmt
            clippy
            # a local-checkout override of either core still needs its
            # toolchain; the pinned default path does not.
            go
            # backends under comparison
            git
            restic
            nix
            # local S3-compatible object store + client for the blob group
            garage
            minio-client
            # exploring recorded results
            (pythonEnv pkgs)
            # helpers used by run.sh
            jq
            coreutils
          ];
        };
      });

      formatter = eachSystem (_system: pkgs:
        pkgs.writeShellScriptBin "fmt" ''
          exec ${pkgs.rustfmt}/bin/cargo-fmt --all
        '');
    };
}
