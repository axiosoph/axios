#!/usr/bin/env bash
# Phase-2 false-clear-floor runner: install a consumer package, run its
# debian/tests autopkgtest twice under baseline libssl3 (3.0.15-1~deb12u1)
# and twice under swapped libssl3 (3.0.16-1~deb12u1), from inside the
# iface-proxy-base:old container.
#
# Usage (run INSIDE an iface-proxy-base:old container -- see
# fetch_snapshot_rootfs.sh -- with /pins mounted from ../../data/libs,
# the two pinned .deb files fetched by ../fetch_libs_and_index.sh):
#   run_consumer_test.sh <binary-package> <autopkgtest-name>
#
# Prints RESULT:<phase><run>:<PASS|FAIL> lines the harness on the host
# greps for; full test output goes to stdout for the log.
set -uo pipefail

pkg="$1"
testname="$2"

echo "=== installing $pkg ==="
apt-get install -y "$pkg" || { echo "RESULT:install:FAIL"; exit 1; }

echo "=== fetching source for $pkg (to get debian/tests/) ==="
mkdir -p /src && cd /src
apt-get source "$pkg" 2>&1 | tail -20
srcdir=$(find . -maxdepth 1 -mindepth 1 -type d | head -1)
cd "$srcdir" || { echo "RESULT:source:FAIL"; exit 1; }

ctl="debian/tests/control"
if [ ! -f "$ctl" ]; then
	echo "RESULT:no-autopkgtest:FAIL"
	exit 1
fi

# Depends: for this specific Tests: stanza only (a control file can have
# several stanzas with different testnames/deps).
deps=$(awk -v t="$testname" '
  /^Tests:/ { split($0, a, ":"); n=a[2]; gsub(/^[ \t]+|[ \t]+$/,"",n); split(n, ts, /[ ,]+/); ok=0; for (i in ts) if (ts[i]==t) ok=1 }
  ok && /^Depends:/ { print; ok=0 }
' "$ctl" | sed 's/^Depends://')

# @builddeps@ means the test needs this source package's Build-Depends
# (it compiles something); only pull those in when actually declared --
# for many consumers (e.g. a daemon's own smoke test) the test suite
# only needs the already-installed binary package (@) plus a short extra
# list, and running build-dep unconditionally was pulling in large,
# unrelated toolchains (observed: kea-ctrl-agent's smoke-tests need only
# "kea, curl, jq", but build-dep pulled the full C++/boost isc-kea build
# closure for no reason -- multiplied across 9 consumers, that's the
# difference between the run finishing in the timebox and not).
if echo "$deps" | grep -q '@builddeps@'; then
	echo "=== build-dep for $pkg (test declares @builddeps@) ==="
	apt-get build-dep -y "$pkg" 2>&1 | tail -20
fi

extra=$(echo "$deps" | tr ',' '\n' | sed 's/^ *//;s/ *$//' | grep -v '^@' | grep -v '^$' | cut -d' ' -f1)
if [ -n "$extra" ]; then
	echo "=== extra test deps: $extra ==="
	apt-get install -y $extra 2>&1 | tail -20
fi

run_test() {
	local label="$1"
	echo "--- $label: running debian/tests/$testname ---"
	# Run under the script's own shebang (chmod +x + direct exec), not
	# forced through `sh` (dash) -- some autopkgtests (e.g. kea-ctrl-agent's
	# smoke-tests) use bash-only features like `set -o pipefail` and abort
	# immediately under dash, which is a harness artifact, not a real
	# result, and would silently masquerade as a stable "FAIL" in both
	# phases.
	chmod +x "debian/tests/$testname"
	# Real autopkgtest infra provides a scratch dir via $AUTOPKGTEST_TMP;
	# some test scripts (e.g. libserf-1-1's upstream-tests) `cd` there
	# before copying files out of the source tree, and fail with "same
	# file" cp errors without it.
	if AUTOPKGTEST_TMP=$(mktemp -d) "./debian/tests/$testname"; then
		echo "RESULT:$label:PASS"
	else
		echo "RESULT:$label:FAIL"
	fi
}

echo "=== BASELINE: installing libssl3 3.0.15-1~deb12u1 (phase-1 old provider) ==="
dpkg -i /pins/libssl3_3.0.15-1~deb12u1_amd64.deb
dpkg -l libssl3 | tail -1
run_test "baseline1"
run_test "baseline2"

echo "=== SWAPPED: installing libssl3 3.0.16-1~deb12u1 (phase-1 new provider) ==="
dpkg -i /pins/libssl3_3.0.16-1~deb12u1_amd64.deb
dpkg -l libssl3 | tail -1
run_test "swapped1"
run_test "swapped2"
