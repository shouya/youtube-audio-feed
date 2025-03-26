{
  inputs = {
    naersk.url = "github:nix-community/naersk/master";
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    utils.url = "github:numtide/flake-utils";
    fenix.url = "github:nix-community/fenix";
  };

  outputs = { self, nixpkgs, utils, naersk, fenix }:
    utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
        buildTarget = "x86_64-unknown-linux-musl";
        devTarget = "x86_64-unknown-linux-gnu";
        toolchain = with fenix.packages.${system};
          combine [
            stable.clippy
            stable.rust-analyzer
            stable.cargo
            stable.rustfmt
            stable.rust-src
            targets.${devTarget}.stable.rust-std
            targets.${buildTarget}.stable.rust-std
          ];
        naersk-lib = pkgs.callPackage naersk {};
      in
      {
        defaultPackage = naersk-lib.buildPackage ./.;
        devShell = with pkgs; mkShell {
          buildInputs = [
            toolchain
            flyctl
            podman
          ];
          RUST_SRC_PATH = "${toolchain}/lib/rustlib/src/rust/library";
        };
      }
    );
}
