let
  fenix = import (builtins.fetchTarball {
    url = "https://github.com/nix-community/fenix/archive/8a42e00e442d416e6c838fc6b40240da65aacbcd.tar.gz";
    sha256 = "sha256:0z6d6gr35ly1haa89yk8zss11ca33naxnp3l2i63p73jaw53g8xi";
  }) { inherit pkgs; };
  pkgs = import (builtins.fetchTarball {
    url = "https://github.com/nixos/nixpkgs/archive/fb7944c166a3b630f177938e478f0378e64ce108.tar.gz";
    sha256 = "sha256:1k5rlkipyc4n7jk8nfmzm1rg3i94zmr90k41yplxhnrb3fkk808j";
  }) { };
  toolchain = fenix.fromToolchainFile { file = ./rust-toolchain.toml; };
in
with pkgs;
mkShell {
  RUST_SRC_PATH = "${toolchain}/lib/rustlib/src/rust/library";
  LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [
    pkgs.stdenv.cc.cc.lib
  ];
  packages = [
    protobuf
    capnproto
    toolchain
    cargo-fuzz
  ];
}
