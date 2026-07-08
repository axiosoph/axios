#!/usr/bin/env bash
# Run the phase-2 false-clear-floor sample: for each (package, testname)
# pair, spin up a fresh container from iface-proxy-base:old, install the
# package, run its debian/tests/<testname> autopkgtest twice under
# baseline libssl3 then twice under swapped libssl3, log everything.
set -uo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
mkdir -p "$here/work/phase2-logs"

# package:testname pairs, seed-42 order (rsync already validated in the
# pilot run, work/rsync-pilot-full.log -- not re-run here).
pairs=(
	"kea-ctrl-agent:smoke-tests"
	"freeradius:freeradius"
	"libsasl2-modules:pluginviewer"
	"unbound-host:runzones"
	"syslog-ng-mod-sql:basic"
	"r-cran-openssl:run-unit-test"
	"libserf-1-1:upstream-tests"
	"rspamd:configcheck"
	"gsocket:upstream-tests"
)

for pair in "${pairs[@]}"; do
	pkg="${pair%%:*}"
	test="${pair##*:}"
	log="$here/work/phase2-logs/${pkg}.log"
	echo "########## $pkg ($test) ##########" | tee "$log"
	timeout 900 podman run --rm \
		-v "$here/data/libs:/pins:ro,Z" \
		-v "$here/scripts/phase2/run_consumer_test.sh:/run_consumer_test.sh:ro,Z" \
		iface-proxy-base:old bash /run_consumer_test.sh "$pkg" "$test" >>"$log" 2>&1
	echo "exit=$? for $pkg" | tee -a "$log"
done

echo "BATCH DONE"
