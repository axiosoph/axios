#!/usr/bin/env bash
# Extract every sampled consumer .deb and record which of its ELF files
# link libssl.so.3/libcrypto.so.3. Writes data/elf-manifest.tsv:
#   package  elf_path  needed_libs (comma-separated)
# Packages with zero qualifying ELFs still get a marker line with an
# empty elf_path so downstream tooling can count them as no-elf.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$here"

manifest="data/elf-manifest.tsv"
: >"$manifest"

for deb in data/consumers/*.deb; do
	base="$(basename "$deb" .deb)"
	# package name is everything before the first underscore-version split;
	# derive from the sample CSV instead, matched by filename, to avoid
	# guessing at version-string underscores.
	dest="work/extracted/$base"
	mkdir -p "$dest"
	if [[ -z "$(ls -A "$dest" 2>/dev/null)" ]]; then
		scripts/extract_deb.sh "$deb" "$dest"
	fi
	hits="$(python3 scripts/find_qualifying_elfs.py "$dest" || true)"
	if [[ -z "$hits" ]]; then
		echo -e "${base}\t\t" >>"$manifest"
	else
		while IFS=$'\t' read -r elfpath needed; do
			echo -e "${base}\t${elfpath}\t${needed}" >>"$manifest"
		done <<<"$hits"
	fi
	echo "done: $base"
done
