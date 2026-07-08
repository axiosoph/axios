#!/usr/bin/env python3
"""Classify each sampled consumer under:

  A) symbol-level satisfaction (objdump -T undefined-symbol/version match)
  B) ABI-level satisfaction (libabigail abicompat)

against an old/new pair of libssl.so.3 + libcrypto.so.3.

Reads data/elf-manifest.tsv (package, elf-path, needed-libs) and
data/consumers-sample.csv (package, version, ...). Writes
phase1-results.csv.
"""
import csv
import re
import subprocess
import sys
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LIB_OLD = {
    "libssl.so.3": ROOT / "work/lib-old/usr/lib/x86_64-linux-gnu/libssl.so.3",
    "libcrypto.so.3": ROOT / "work/lib-old/usr/lib/x86_64-linux-gnu/libcrypto.so.3",
}
LIB_NEW = {
    "libssl.so.3": ROOT / "work/lib-new/usr/lib/x86_64-linux-gnu/libssl.so.3",
    "libcrypto.so.3": ROOT / "work/lib-new/usr/lib/x86_64-linux-gnu/libcrypto.so.3",
}
# abicompat 2.5.0 crashes (std::logic_error: basic_string: construction
# from null) when it tries to auto-discover debug-info directories and
# finds none. Pointing --appd/--libd1/--libd2 at any existing directory
# (even one with no matching debug info) avoids the crash entirely and
# produces identical results to the no-debug-info case. See
# PHASE1-REPORT.md "abicompat tripwire" for the verification trail.
NODEBUG_DIR = ROOT / "work/nodebug"
# Resolved once via `nix shell nixpkgs#libabigail --command which abicompat`
# and pinned here to avoid ~1s of nix-shell overhead per invocation across
# ~250 calls. Falls back to plain `abicompat` on PATH if the store path is
# gone (e.g. GC'd) so the script stays runnable without edits.
ABICOMPAT = "/nix/store/90jrirjd87rw6kkijs192d29hxifsx2r-libabigail-2.5-bin/bin/abicompat"
if not Path(ABICOMPAT).exists():
    ABICOMPAT = "abicompat"

ADDR_RE = re.compile(r"^[0-9a-fA-F]+$")
HEX_RE = re.compile(r"^[0-9a-fA-F]+$")


def objdump_T(path):
    return subprocess.run(
        ["objdump", "-T", str(path)], capture_output=True, text=True, timeout=60
    ).stdout


def parse_symbols(dump_text, want_undefined):
    """Return set of (name, version_or_None) tuples.

    objdump -T rows are whitespace-separated with a variable number of
    flag columns, so we anchor on the fixed suffix instead: the last
    token is always the symbol name, and the token before it is the
    version IF it isn't a bare hex value (which means "the size field",
    i.e. this symbol carries no version tag). Defined symbols show their
    version bare (e.g. "OPENSSL_3.0.0 EVP_MD_CTX_new"); undefined symbols
    show it parenthesized (e.g. "(OPENSSL_3.0.0) EVP_MD_CTX_new").
    """
    out = set()
    for line in dump_text.splitlines():
        is_undef = "*UND*" in line
        if is_undef != want_undefined:
            continue
        toks = line.split()
        if len(toks) < 4 or not ADDR_RE.match(toks[0]):
            continue
        name = toks[-1]
        prev = toks[-2]
        if HEX_RE.match(prev):
            version = None
        else:
            version = prev.strip("()")
        out.add((name, version))
    return out


def defined_symbol_set(lib_paths):
    s = set()
    for p in lib_paths:
        s |= parse_symbols(objdump_T(p), want_undefined=False)
    return s


def classify_a(elf_path, old_defined, new_defined):
    und = parse_symbols(objdump_T(elf_path), want_undefined=True)
    # attribute to openssl iff (name, version) is defined by the OLD
    # provider -- i.e. this is where the symbol must have resolved from
    # at build/link time.
    attributed = {s for s in und if s in old_defined}
    missing = sorted(s for s in attributed if s not in new_defined)
    return attributed, missing


def classify_b(elf_path, needed_libs):
    """Run abicompat for each needed lib name; return list of
    (lib_name, verdict, detail) tuples. verdict in {compat, incompat, error}."""
    results = []
    for lib_name in needed_libs:
        old_lib = LIB_OLD[lib_name]
        new_lib = LIB_NEW[lib_name]
        cmd = [
            ABICOMPAT,
            "--appd", str(NODEBUG_DIR),
            "--libd1", str(NODEBUG_DIR),
            "--libd2", str(NODEBUG_DIR),
            str(elf_path), str(old_lib), str(new_lib),
        ]
        try:
            proc = subprocess.run(
                cmd, capture_output=True, text=True, timeout=120,
            )
        except subprocess.TimeoutExpired:
            results.append((lib_name, "error", "timeout"))
            continue
        stdout = proc.stdout.strip()
        stderr = proc.stderr.strip()
        if proc.returncode == 0 and not stdout:
            results.append((lib_name, "compat", ""))
        elif stdout:
            # non-empty report = abicompat found an incompatibility
            summary = " | ".join(stdout.splitlines()[:6]).replace("\n", " ")
            results.append((lib_name, "incompat", summary[:500]))
        else:
            summary = (stderr or f"exit={proc.returncode}").replace("\n", " ")
            results.append((lib_name, "error", summary[:500]))
    return results


def main():
    manifest = defaultdict(list)  # package -> [(elf_path, [libs]), ...]
    with open(ROOT / "data/elf-manifest.tsv") as fh:
        for line in fh:
            parts = line.rstrip("\n").split("\t")
            pkg = parts[0]
            elf = parts[1] if len(parts) > 1 else ""
            needed = parts[2].split(",") if len(parts) > 2 and parts[2] else []
            if elf:
                manifest[pkg].append((elf, needed))
            else:
                manifest.setdefault(pkg, [])

    versions = {}
    with open(ROOT / "data/consumers-sample.csv") as fh:
        for row in csv.DictReader(fh):
            versions[row["package"]] = row["version"]

    print("building defined-symbol sets for old/new providers...", file=sys.stderr)
    old_defined = defined_symbol_set([LIB_OLD["libssl.so.3"], LIB_OLD["libcrypto.so.3"]])
    new_defined = defined_symbol_set([LIB_NEW["libssl.so.3"], LIB_NEW["libcrypto.so.3"]])
    print(f"old defined: {len(old_defined)}  new defined: {len(new_defined)}", file=sys.stderr)

    NODEBUG_DIR.mkdir(parents=True, exist_ok=True)

    rows = []
    # deterministic order: by extracted-dir basename maps back via sample CSV order
    pkg_order = sorted(manifest.keys())
    total = len(pkg_order)
    for i, extracted_name in enumerate(pkg_order, 1):
        elfs = manifest[extracted_name]
        # extracted_name is the deb basename (pkg_version_arch); recover
        # the plain package name by matching against versions dict prefix
        pkg_name = None
        for name in versions:
            if extracted_name.startswith(name + "_"):
                pkg_name = name
                break
        if pkg_name is None:
            pkg_name = extracted_name.split("_")[0]
        version = versions.get(pkg_name, "")

        print(f"[{i}/{total}] {pkg_name}  elfs={len(elfs)}", file=sys.stderr)

        if not elfs:
            rows.append({
                "package": pkg_name, "version": version, "elf_count": 0,
                "a_verdict": "no-elf", "a_missing": "",
                "b_verdict": "no-elf", "b_detail": "",
                "agreement": "n/a",
            })
            continue

        all_missing = []
        a_cleared = True
        b_cleared = True
        b_error = False
        b_details = []
        for elf_path, needed in elfs:
            attributed, missing = classify_a(elf_path, old_defined, new_defined)
            if missing:
                a_cleared = False
                all_missing.extend(f"{n}@{v}" for n, v in missing)

            b_res = classify_b(elf_path, needed)
            for lib_name, verdict, detail in b_res:
                if verdict == "incompat":
                    b_cleared = False
                    b_details.append(f"{Path(elf_path).name}/{lib_name}: {detail}")
                elif verdict == "error":
                    b_error = True
                    b_details.append(f"{Path(elf_path).name}/{lib_name}: ERROR {detail}")

        a_verdict = "cleared" if a_cleared else "frontier"
        if not b_cleared:
            b_verdict = "incompat"
        elif b_error:
            b_verdict = "error"
        else:
            b_verdict = "compat"

        a_bool = a_verdict == "cleared"
        b_bool = b_verdict == "compat"
        if b_verdict == "error":
            agreement = "n/a-error"
        else:
            agreement = "agree" if a_bool == b_bool else "DISAGREE"

        rows.append({
            "package": pkg_name, "version": version, "elf_count": len(elfs),
            "a_verdict": a_verdict,
            "a_missing": ";".join(sorted(set(all_missing))),
            "b_verdict": b_verdict,
            "b_detail": " || ".join(b_details)[:1000],
            "agreement": agreement,
        })

    out_path = ROOT / "phase1-results.csv"
    with open(out_path, "w", newline="") as fh:
        w = csv.DictWriter(fh, fieldnames=[
            "package", "version", "elf_count", "a_verdict", "a_missing",
            "b_verdict", "b_detail", "agreement",
        ], lineterminator="\n")
        w.writeheader()
        for r in sorted(rows, key=lambda r: r["package"]):
            w.writerow(r)

    print(f"wrote {out_path}", file=sys.stderr)


if __name__ == "__main__":
    main()
