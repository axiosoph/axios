# Reproducible checker environment for the atom-transactions formal models.
#
# One pinned nixpkgs provisions BOTH checkers used by docs/specs:
#   * tlaplus (TLC)  -- docs/specs/tla/*.tla
#   * alloy    (Alloy Analyzer 5.1.0) -- docs/specs/alloy/*.als
#   * jre      -- runs both the TLA+ tools and the Alloy jar
#
# The pin matches docs/models/tla/shell.nix so the whole repository's
# model-checking toolchain is a single, cache-hittable revision.
#
# Usage:
#   nix-shell docs/specs/shell.nix --run docs/specs/run_model_check.sh
let
  pkgs = import (builtins.fetchTarball {
    url = "https://github.com/nixos/nixpkgs/archive/fb7944c166a3b630f177938e478f0378e64ce108.tar.gz";
    sha256 = "sha256:1k5rlkipyc4n7jk8nfmzm1rg3i94zmr90k41yplxhnrb3fkk808j";
  }) { };
in
pkgs.mkShell {
  packages = with pkgs; [
    jre
    tlaplus
    alloy
  ];
  # ALLOY_JAR lets the runner locate the headless SimpleCLI driver without
  # hardcoding a /nix/store path.
  ALLOY_JAR = "${pkgs.alloy}/share/alloy/alloy5.jar";
  shellHook = ''
    echo "atom-transactions model-checking environment (TLC + Alloy 5.1.0)"
  '';
}
