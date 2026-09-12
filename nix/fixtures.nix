# Deterministic Nix store fixtures for the "nix" scenario group.
#
# The closure shape is the point: `app` pulls in two mid-level paths that
# *share* a leaf (`lib-shared`), every path carries executable files and
# symlinks (including one dangling and one absolute store symlink), and two
# bulk paths supply incompressible and compressible payload so copy costs are
# measurable. `gen2` is the second generation: the same closure except for one
# rewritten bulk path and one new leaf, which is what makes an incremental
# push/pull measurement meaningful.
#
# All payload bytes come from an AES-CTR keystream over /dev/zero, i.e. they
# are a pure function of the seed string -- no /dev/urandom, no timestamps, no
# host paths. Nix itself normalises mtimes and ownership, so a rebuild from
# the same pinned nixpkgs yields the same store paths.
{ runCommand, openssl, coreutils }:

let
  # Shell helpers injected into every fixture builder.
  #   stream SEED BYTES  -> deterministic pseudorandom bytes on stdout
  #   filler SEED BYTES  -> deterministic, highly compressible text on stdout
  helpers = ''
    stream() {
      { ${openssl}/bin/openssl enc -aes-256-ctr -nosalt \
          -pass "pass:amber-bench-$1" -in /dev/zero 2>/dev/null \
        | ${coreutils}/bin/head -c "$2"; } || true
    }
    filler() {
      { ${coreutils}/bin/yes \
          "amber-cas-bench filler for seed $1 -- padding padding padding" \
        | ${coreutils}/bin/head -c "$2"; } || true
    }
  '';

  mk = name: { deps ? [ ], script }:
    runCommand "amber-bench-${name}" { inherit deps; } ''
      set -uo pipefail
      ${helpers}
      mkdir -p "$out"
      # Recording the dependency store paths in a plain file is what makes
      # Nix register them as references of this output.
      : > "$out/references.txt"
      for d in $deps; do echo "$d" >> "$out/references.txt"; done
      ${script}
    '';

  # unit is the per-path payload size in bytes; it scales the whole fixture.
  fixture = { suffix, unit }:
    let
      half = unit / 2;
      double = unit * 2;

      libShared = mk "lib-shared${suffix}" {
        script = ''
          mkdir -p "$out/lib" "$out/bin" "$out/share"
          stream shared-a ${toString unit} > "$out/lib/shared-a.bin"
          filler shared-t ${toString unit} > "$out/share/shared.txt"
          printf '#!/bin/sh\necho amber-bench shared\n' > "$out/bin/shared-tool"
          chmod +x "$out/bin/shared-tool"
          ln -s ../lib/shared-a.bin "$out/bin/shared-a.link"
          ln -s /does/not/exist "$out/share/dangling.link"
        '';
      };

      leaf = n: seed: mk "lib-${n}${suffix}" {
        script = ''
          mkdir -p "$out/lib" "$out/bin"
          stream ${seed} ${toString unit} > "$out/lib/${n}.bin"
          printf '#!/bin/sh\necho amber-bench ${n}\n' > "$out/bin/${n}-tool"
          chmod +x "$out/bin/${n}-tool"
          ln -s ${n}-tool "$out/bin/${n}-alias"
        '';
      };

      libCore = leaf "core" "core-seed";
      libExtra = leaf "extra" "extra-seed";
      libLate = leaf "late" "late-seed";

      mid = n: dep: mk "mid-${n}${suffix}" {
        deps = [ dep libShared ];
        script = ''
          mkdir -p "$out/bin" "$out/lib"
          stream mid-${n} ${toString half} > "$out/lib/mid-${n}.bin"
          printf '#!/bin/sh\nexec %s/bin/shared-tool "$@"\n' "${libShared}" \
            > "$out/bin/mid-${n}"
          chmod +x "$out/bin/mid-${n}"
          ln -s ${libShared}/bin/shared-tool "$out/bin/shared-tool"
        '';
      };

      midA = mid "a" libCore;
      midB = mid "b" libExtra;

      dataBlob = seed: mk "data-blob-${seed}${suffix}" {
        script = ''
          mkdir -p "$out/data"
          for i in 0 1 2 3; do
            stream ${seed}-$i ${toString double} > "$out/data/blob-$i.bin"
          done
        '';
      };

      dataText = seed: mk "data-text-${seed}${suffix}" {
        script = ''
          mkdir -p "$out/data"
          for i in 0 1 2 3; do
            filler ${seed}-$i ${toString double} > "$out/data/text-$i.txt"
          done
        '';
      };

      app = n: extra: mk "app-${n}${suffix}" {
        deps = [ midA midB ] ++ extra;
        script = ''
          mkdir -p "$out/bin"
          printf '#!/bin/sh\nexec %s/bin/mid-a "$@"\n' "${midA}" > "$out/bin/app"
          chmod +x "$out/bin/app"
          ln -s ${midB}/bin/mid-b "$out/bin/mid-b"
          stream app-${n} ${toString unit} > "$out/bin/app.payload"
        '';
      };

      gen1 = app "gen1" [ (dataBlob "g1") (dataText "g1") ];
      gen2 = app "gen2" [ (dataBlob "g1") (dataText "g2") libLate ];
    in
    runCommand "amber-bench-nix-fixtures${suffix}" { } ''
      mkdir -p "$out"
      ln -s ${gen1} "$out/gen1"
      ln -s ${gen2} "$out/gen2"
    '';
in
{
  smoke = fixture { suffix = "-smoke"; unit = 256 * 1024; };
  standard = fixture { suffix = "-standard"; unit = 8 * 1024 * 1024; };
  large = fixture { suffix = "-large"; unit = 64 * 1024 * 1024; };
}
