{
  description = "flluf — a minimal compiled language";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      rust-overlay,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };

        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [
            "rust-src"
            "rust-analyzer"
          ];
          targets = [ "x86_64-pc-windows-gnu" ];
        };

        mingwCC = pkgs.pkgsCross.mingwW64.stdenv.cc;
        mingwPthreads = pkgs.pkgsCross.mingwW64.windows.pthreads;
      in
      {
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            rustToolchain
            gcc
            pkg-config
            mingwCC
            mingwPthreads
          ];

          CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = "${mingwCC}/bin/x86_64-w64-mingw32-gcc";

          shellHook = ''
            echo "flluf dev shell (with Windows cross-compilation support)"
            echo "  rustc   : $(rustc --version)"
            echo "  cargo   : $(cargo --version)"
            echo "  mingw   : $(${mingwCC}/bin/x86_64-w64-mingw32-gcc --version | head -1)"
            echo ""
            echo "Для сборки под Windows используйте:"
            echo "  cargo build --target x86_64-pc-windows-gnu --release"
          '';
        };

        packages.default = rustToolchain;
      }
    );
}
