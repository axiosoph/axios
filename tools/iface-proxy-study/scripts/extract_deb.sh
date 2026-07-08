#!/usr/bin/env bash
# Extract a .deb's data.tar.* into a target directory. No dpkg required:
# .deb is an `ar` archive containing control.tar.*, data.tar.*, debian-binary.
set -euo pipefail

deb="${1:?usage: extract_deb.sh <deb-path> <dest-dir>}"
dest="${2:?usage: extract_deb.sh <deb-path> <dest-dir>}"

mkdir -p "$dest"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

ar x --output="$work" "$deb"
data_member="$(ls "$work" | grep '^data\.tar')"
tar -xf "$work/$data_member" -C "$dest"
