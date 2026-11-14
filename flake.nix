
{
  description = "Wallace Flake";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

    flake-utils.url = "github:numtide/flake-utils";

    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    naersk = {
      url = "github:nix-community/naersk";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      fenix,
      naersk,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
        };

        toolchain = fenix.packages.${system}.latest.toolchain;

        naersk-lib = pkgs.callPackage naersk {
          cargo = toolchain;
          rustc = toolchain;
        };

        llvm = pkgs.llvmPackages;

        buildDeps = with pkgs; [
          toolchain
          pkg-config
          openssl
          llvm.libclang.lib
          llvm.llvm.lib
          stdenv.cc.cc.lib
          alsa-lib
        ];

        devShellDeps = with pkgs; [
          rust-analyzer
          git
          gnumake
        ];

        buildEnv = {
          LIBCLANG_PATH = "${llvm.libclang.lib}/lib";

          ALSA_PLUGIN_DIR = "${pkgs.pipewire}/lib/alsa-lib";

          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [
            llvm.libclang.lib
            llvm.llvm.lib
            pkgs.stdenv.cc.cc.lib
          ];

          RUST_SRC_PATH = "${toolchain}/lib/rustlib/src/rust/library";
        };
      in
      {
        packages.default = naersk-lib.buildPackage ({
          src = ./.;
          nativeBuildInputs = buildDeps;
        } // buildEnv);

        devShells.default = pkgs.mkShell ({
          packages = buildDeps ++ devShellDeps;
        } // buildEnv);
      }
    );
}
