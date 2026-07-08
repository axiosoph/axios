#!/usr/bin/env python3
"""Parse a Debian Packages(.xz) index and enumerate reverse-dependencies of
a target binary package (libssl3), then produce a deterministic seeded
sample.

Usage: enumerate_consumers.py <Packages.xz> <target-pkg> <sample-size> <seed>

Writes:
  data/consumers-all.txt      -- every reverse-dep package name, one per line
  data/consumers-sample.csv   -- sampled package name + version + filename + sha256
"""
import lzma
import random
import sys
import csv
from pathlib import Path


def parse_packages(path):
    """Yield dicts of Packages-file stanzas."""
    text = lzma.open(path, "rt", encoding="utf-8", errors="replace").read()
    stanzas = text.split("\n\n")
    for stanza in stanzas:
        if not stanza.strip():
            continue
        fields = {}
        cur_key = None
        for line in stanza.splitlines():
            if line.startswith((" ", "\t")):
                # continuation line
                if cur_key:
                    fields[cur_key] += "\n" + line
                continue
            if ":" in line:
                key, _, val = line.partition(":")
                cur_key = key.strip()
                fields[cur_key] = val.strip()
        if fields:
            yield fields


def depends_on(fields, target):
    for depkey in ("Depends", "Pre-Depends"):
        raw = fields.get(depkey, "")
        if not raw:
            continue
        # split on commas (AND), then on | (OR alternatives)
        for group in raw.split(","):
            alts = [a.strip() for a in group.split("|")]
            for alt in alts:
                name = alt.split("(")[0].strip()
                if name == target:
                    return True
    return False


def main():
    packages_path, target, sample_size, seed = sys.argv[1:5]
    sample_size = int(sample_size)
    seed = int(seed)

    out_dir = Path(__file__).resolve().parent.parent / "data"
    out_dir.mkdir(exist_ok=True)

    consumers = []
    for fields in parse_packages(packages_path):
        if depends_on(fields, target):
            consumers.append(fields)

    # dedupe by package name, keep first (Packages index may list multiple
    # versions per arch in some cases; amd64-only index here, so this is
    # mostly a no-op safety net)
    by_name = {}
    for f in consumers:
        name = f.get("Package")
        if name and name not in by_name:
            by_name[name] = f

    names_sorted = sorted(by_name.keys())
    with open(out_dir / "consumers-all.txt", "w") as fh:
        for n in names_sorted:
            fh.write(n + "\n")

    rng = random.Random(seed)
    shuffled = names_sorted[:]
    rng.shuffle(shuffled)
    sample = sorted(shuffled[:sample_size])

    with open(out_dir / "consumers-sample.csv", "w", newline="") as fh:
        w = csv.writer(fh, lineterminator="\n")
        w.writerow(["package", "version", "filename", "sha256"])
        for name in sample:
            f = by_name[name]
            w.writerow([
                name,
                f.get("Version", ""),
                f.get("Filename", ""),
                f.get("SHA256", ""),
            ])

    print(f"total reverse-deps of {target}: {len(names_sorted)}")
    print(f"sampled: {len(sample)} (seed={seed})")


if __name__ == "__main__":
    main()
