#!/usr/bin/env bash
# Probe candidate packages (in sample order) for real autopkgtest coverage,
# without needing build-essential. Prints, per package, whether
# debian/tests/control exists and its Tests: stanza.
set -uo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
pkgs=("$@")

podman run --rm iface-proxy-base:old bash -c "
set -u
for p in ${pkgs[*]}; do
  echo \"=== \$p ===\"
  mkdir -p /src/\$p && cd /src/\$p
  out=\$(apt-get source \$p 2>&1)
  if [ \$? -ne 0 ]; then
    echo \"  SOURCE FETCH FAILED\"
    continue
  fi
  ctl=\$(find . -maxdepth 4 -path '*/debian/tests/control' 2>/dev/null | head -1)
  if [ -z \"\$ctl\" ]; then
    echo \"  no debian/tests/control\"
  else
    echo \"  HAS AUTOPKGTEST: \$ctl\"
    sed 's/^/    /' \"\$ctl\"
  fi
  cd /
  rm -rf /src/\$p
done
"
