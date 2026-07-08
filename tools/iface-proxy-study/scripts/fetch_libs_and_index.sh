#!/usr/bin/env bash
# Fetch the pinned libssl3 old/new .debs and the bookworm Packages.xz
# index from snapshot.debian.org, verifying sha256 against sources.lock.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
mkdir -p "$here/data/libs" "$here/data/index"

fetch() {
	local url="$1" dest="$2" sha256="$3"
	if [[ -f "$dest" ]] && [[ "$(sha256sum "$dest" | cut -d' ' -f1)" == "$sha256" ]]; then
		echo "skip (cached, verified): $(basename "$dest")"
		return
	fi
	echo "fetch: $(basename "$dest")"
	curl -sfL -o "$dest" "$url"
	got="$(sha256sum "$dest" | cut -d' ' -f1)"
	if [[ "$got" != "$sha256" ]]; then
		echo "SHA256 MISMATCH for $dest: expected $sha256 got $got" >&2
		exit 1
	fi
}

fetch \
	"https://snapshot.debian.org/archive/debian/20241101T025324Z/pool/main/o/openssl/libssl3_3.0.15-1~deb12u1_amd64.deb" \
	"$here/data/libs/libssl3_3.0.15-1~deb12u1_amd64.deb" \
	"d7897e6c55a8d9e229dcf16b0b1d472d7f7be741b2b3b2ac624908ff63215a93"

fetch \
	"https://snapshot.debian.org/archive/debian/20250426T204242Z/pool/main/o/openssl/libssl3_3.0.16-1~deb12u1_amd64.deb" \
	"$here/data/libs/libssl3_3.0.16-1~deb12u1_amd64.deb" \
	"eaa2bab2130820f09361dc8186ddeb11d2a18ec5e5e3806f24414d5d8065a57a"

fetch \
	"https://snapshot.debian.org/archive/debian/20241101T025324Z/dists/bookworm/main/binary-amd64/Packages.xz" \
	"$here/data/index/Packages.xz" \
	"aea3ea5e8161c894b6374538a9f81f5e334b9a5d4f01e40e3e6f636e75023de0"

echo "extracting libssl3 pair into work/lib-old and work/lib-new..."
mkdir -p "$here/work/lib-old" "$here/work/lib-new"
bash "$here/scripts/extract_deb.sh" "$here/data/libs/libssl3_3.0.15-1~deb12u1_amd64.deb" "$here/work/lib-old"
bash "$here/scripts/extract_deb.sh" "$here/data/libs/libssl3_3.0.16-1~deb12u1_amd64.deb" "$here/work/lib-new"
