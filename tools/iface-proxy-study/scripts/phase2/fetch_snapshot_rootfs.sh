#!/usr/bin/env bash
# Build iface-proxy-base:snapshot-raw: a bookworm rootfs debootstrapped
# directly from the snapshot.debian.org instant that matches phase-1's
# "old provider" (libssl3 3.0.15-1~deb12u1, see ../../sources.lock).
#
# Why debootstrap instead of `FROM docker.io/library/debian:bookworm`:
# that rolling image is rebuilt at whatever bookworm point release is
# current on the day it's pulled (2026, in our case), so several of its
# preinstalled packages (libc6, perl-base, gcc-12-base, ...) are AHEAD of
# the 2024-11-01 snapshot instant. apt will not auto-downgrade an
# already-installed higher version (Candidate selection always prefers
# the higher version, even one no longer in any configured repo), and
# forcing it package-by-package breaks Debian's co-versioned toolchain
# packages (cpp-12/gcc-12/g++-12/libgcc-12-dev, ... all require an exact
# "=" version match on their gcc-12-base sibling). debootstrapping the
# whole floor from the pinned snapshot sidesteps the problem entirely.
#
# Why fakechroot: real `debootstrap` needs to create device nodes
# (mknod) inside its target, which the kernel refuses inside a rootless
# user namespace (mounts made there always get MS_NODEV) regardless of
# capabilities -- confirmed by testing --privileged and an explicit tmpfs
# mount at the target, both still refused. `fakeroot fakechroot
# debootstrap --variant=fakechroot` avoids needing mknod/chroot() at all
# (LD_PRELOAD-intercepted file ops instead), and works under ordinary
# rootless podman.
#
# fakechroot leaves two categories of artifacts that must be fixed up
# before the tarball is a valid OCI rootfs:
#  1. `/proc` comes out as an absolute symlink to the HOST's real /proc
#     (fakechroot can't do a real chroot, so it left the debootstrap
#     script's literal `ln -s /proc proc` untouched) -- runtimes refuse
#     this (need a real directory to mount proc onto).
#  2. Any absolute symlink debootstrap created pointing into its own
#     target path (e.g. `/rootfs/lib/x86_64-linux-gnu/...`) is recorded
#     literally, since fakechroot doesn't rewrite symlink *contents* --
#     only real chroot() would make "/lib/..." naturally resolve inside
#     the target. Every such symlink needs its `/rootfs` prefix stripped.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
work="$here/work"
mkdir -p "$work"

echo "== debootstrapping bookworm from the 20241101T025324Z snapshot (fakechroot, in a throwaway container) =="
podman run --rm -v "$work:/out:Z" docker.io/library/debian:bookworm bash -c '
set -e
apt-get update -qq
apt-get install -y -qq debootstrap fakechroot fakeroot ca-certificates >/dev/null
mkdir -p /rootfs
fakeroot fakechroot debootstrap --variant=fakechroot bookworm /rootfs http://snapshot.debian.org/archive/debian/20241101T025324Z/
tar -C /rootfs -cf /out/bookworm-snapshot-rootfs.tar .
'

echo "== fixing up fakechroot artifacts (proc symlink, /rootfs-prefixed absolute symlinks, /tmp ownership) =="
rm -rf "$work/rootfs-fixed"
mkdir -p "$work/rootfs-fixed"
tar -C "$work/rootfs-fixed" -xf "$work/bookworm-snapshot-rootfs.tar"

rm -f "$work/rootfs-fixed/proc"
mkdir -p "$work/rootfs-fixed/proc"

count=0
while IFS= read -r -d '' link; do
	target=$(readlink "$link")
	case "$target" in
	/rootfs/*)
		ln -sfn "${target#/rootfs}" "$link"
		count=$((count + 1))
		;;
	esac
done < <(find "$work/rootfs-fixed" -type l -print0)
echo "fixed $count absolute /rootfs-prefixed symlinks"

rm -f "$work/bookworm-snapshot-rootfs-fixed.tar"
tar -C "$work/rootfs-fixed" -cf "$work/bookworm-snapshot-rootfs-fixed.tar" .

echo "== importing as iface-proxy-base:snapshot-raw =="
podman import "$work/bookworm-snapshot-rootfs-fixed.tar" iface-proxy-base:snapshot-raw

echo "== building iface-proxy-base:old (apt sources + dev/test tooling) on top =="
podman build -f "$here/scripts/phase2/Containerfile.base" -t iface-proxy-base:old "$here"

echo "done: iface-proxy-base:old is ready"
