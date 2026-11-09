{
  description = "kmpl: kepler math programming language";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAll = f: nixpkgs.lib.genAttrs systems (s: f nixpkgs.legacyPackages.${s});
      # `kmpl_codegen` links against this LLVM through `llvm-sys`.
      llvm = pkgs: pkgs.llvmPackages_23.llvm;
      llvmLibs =
        pkgs: with pkgs; [
          libffi
          libxml2
          ncurses
          zlib
          zstd
        ];
    in
    {
      devShells = forAll (pkgs: {
        default = pkgs.mkShell {
          packages = with pkgs; [
            cargo
            rustc
            clippy
            rustfmt
            rust-analyzer
            (llvm pkgs).dev
          ] ++ llvmLibs pkgs;
          RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
          LLVM_SYS_231_PREFIX = "${(llvm pkgs).dev}";
        };
      });

      packages = forAll (pkgs: {
        default = pkgs.rustPlatform.buildRustPackage {
          pname = "kmpl";
          version = "0.1.0";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
          nativeBuildInputs = [ (llvm pkgs).dev ];
          buildInputs = llvmLibs pkgs;
          LLVM_SYS_231_PREFIX = "${(llvm pkgs).dev}";
          cargoBuildFlags = [
            "-p"
            "kmpl"
          ];
          meta = {
            mainProgram = "kmpl";
            license = with pkgs.lib.licenses; [
              mit
              asl20
            ];
          };
        };
      });
    };
}
