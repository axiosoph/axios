let
  pkgs = import (builtins.fetchTarball {
    url = "https://github.com/nixos/nixpkgs/archive/fb7944c166a3b630f177938e478f0378e64ce108.tar.gz";
    sha256 = "sha256:1k5rlkipyc4n7jk8nfmzm1rg3i94zmr90k41yplxhnrb3fkk808j";
  }) { };
  python = pkgs.python3.withPackages (ps: with ps; [
    pytest
  ]);
in
pkgs.mkShell {
  packages = [ python ];
}
