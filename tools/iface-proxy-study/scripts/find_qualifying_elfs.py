#!/usr/bin/env python3
"""Walk an extracted consumer tree and report every ELF file that has a
DT_NEEDED entry for libssl.so.3 or libcrypto.so.3.

Usage: find_qualifying_elfs.py <extracted-root>
Prints one path per line, or nothing if none found.
"""
import subprocess
import sys
from pathlib import Path

TARGETS = {"libssl.so.3", "libcrypto.so.3"}


def is_elf(path: Path) -> bool:
    try:
        with open(path, "rb") as fh:
            return fh.read(4) == b"\x7fELF"
    except OSError:
        return False


def needed_libs(path: Path):
    try:
        out = subprocess.run(
            ["objdump", "-p", str(path)],
            capture_output=True, text=True, timeout=30,
        ).stdout
    except Exception:
        return set()
    needed = set()
    for line in out.splitlines():
        line = line.strip()
        if line.startswith("NEEDED"):
            needed.add(line.split()[1])
    return needed


def main():
    root = Path(sys.argv[1])
    for path in root.rglob("*"):
        if not path.is_file() or path.is_symlink():
            continue
        if not is_elf(path):
            continue
        needed = needed_libs(path)
        hit = needed & TARGETS
        if hit:
            print(f"{path}\t{','.join(sorted(hit))}")


if __name__ == "__main__":
    main()
