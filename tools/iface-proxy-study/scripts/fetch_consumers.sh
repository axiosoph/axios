#!/usr/bin/env bash
# Download every sampled consumer .deb from a pinned snapshot.debian.org
# timestamp, verifying sha256 against the Packages index we parsed them
# from. Idempotent: skips files that already exist with a matching hash.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
snapshot_ts="${1:?usage: fetch_consumers.sh <snapshot-timestamp> <csv>}"
csv="${2:?usage: fetch_consumers.sh <snapshot-timestamp> <csv>}"
out_dir="$here/data/consumers"
mkdir -p "$out_dir"

base="https://snapshot.debian.org/archive/debian/${snapshot_ts}"

tail -n +2 "$csv" | tr -d '\r' | while IFS=, read -r pkg version filename sha256; do
	dest="$out_dir/$(basename "$filename")"
	if [[ -f "$dest" ]]; then
		have="$(sha256sum "$dest" | cut -d' ' -f1)"
		if [[ "$have" == "$sha256" ]]; then
			echo "skip (cached, verified): $pkg"
			continue
		fi
	fi
	echo "fetch: $pkg $version"
	curl -sfL -o "$dest" "${base}/${filename}"
	got="$(sha256sum "$dest" | cut -d' ' -f1)"
	if [[ "$got" != "$sha256" ]]; then
		echo "SHA256 MISMATCH for $pkg: expected $sha256 got $got" >&2
		rm -f "$dest"
		echo "$pkg,$version,FETCH_HASH_MISMATCH" >>"$here/data/fetch-failures.csv"
		continue
	fi
done
