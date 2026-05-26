{ pkgs ? import <nixpkgs> {} }:

pkgs.mkShell {
  buildInputs = with pkgs; [
    cargo
    rustc
    rustfmt
    clippy
    pkg-config
    openssl
    git
  ];
}
