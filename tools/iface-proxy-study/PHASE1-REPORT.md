# Interface-proxy operating characteristics: Phase 1 (primary case)

Status: phase 1 complete. Phase 2 and the contrast cases (zlib, openssl
minor bump, glibc) were explicitly out of scope for this run and were not
executed.

## Question

When a library provider is swapped for a newer point release, what
fraction of its real-world reverse-dependencies can be _cleared_ for the
swap by static interface analysis, and do two different static
classifiers (symbol-level vs ABI-level) agree on which ones?

This phase measures the two clearable fractions and the A/B disagreement
set on a random sample of real Debian binaries. It does not measure
false-clear rate (that requires phase 2: running each cleared consumer's
own tests under both providers).

## Library pair studied

Debian bookworm (12), `libssl3` binary package, amd64, two consecutive
3.0.x point releases:

|                               | version          | openssl upstream | snapshot timestamp |
| ----------------------------- | ---------------- | ---------------- | ------------------ |
| old (provider being replaced) | 3.0.15-1~deb12u1 | 3.0.15           | 2024-11-01         |
| new (replacement)             | 3.0.16-1~deb12u1 | 3.0.16           | 2025-04-26         |

Exact URLs and sha256 pins for both `.deb`s, the bookworm `Packages.xz`
index used for reverse-dependency enumeration, and the 100-consumer
sample (each row carrying its own upstream sha256) are in
`sources.lock` and `data/consumers-sample.csv` in this directory. The
run is reproducible from those pins alone; nothing depends on files
outside this directory tree.

`libssl3` ships two shared objects, `libssl.so.3` and `libcrypto.so.3`.
Every consumer is classified against whichever of the pair it actually
links (checked via `DT_NEEDED`), and only against the ones it links.

## Method

**Reverse-dependency enumeration.** Parsed the bookworm main/binary-amd64
`Packages` index (same snapshot instant as the old library) and collected
every package whose `Depends`/`Pre-Depends` names `libssl3`: **838**
packages. Sorted the names, seeded a Python `random.Random(42)` shuffle,
took the first 100, re-sorted for a deterministic, reviewable sample.

**Download and extraction.** Each sampled package's `.deb` was fetched
from the pinned snapshot timestamp and its sha256 verified against the
Packages index (all 100 verified clean on first fetch). `.deb` files are
`ar` archives; extracted with `ar x --output=` + `tar` (no `dpkg`
required). Every file in the extracted tree was checked for an ELF magic
number, then `objdump -p` was used to read its `DT_NEEDED` list. **173**
ELF files across the 100 packages actually link `libssl.so.3` and/or
`libcrypto.so.3`; the rest of each package's contents (docs, non-ELF
data, unrelated binaries) were not classified, since they can't regress
from this swap.

**Classifier A (symbol-level).** For each qualifying ELF, `objdump -T`
lists its undefined dynamic symbols; each carries a version tag when the
provider versions its exports (e.g. `EVP_MD_CTX_new@OPENSSL_3.0.0`). A
symbol is attributed to the swap iff its exact `(name, version)` pair is
defined by the _old_ provider (proving it's where the consumer actually
resolved it at build time — Debian's openssl packaging versions
essentially everything, 0 unversioned exports found in either
`libssl.so.3` or `libcrypto.so.3`). The consumer is cleared under A iff
every attributed `(name, version)` pair is also defined by the _new_
provider. This is the `dpkg-shlibdeps`/`elfdeps` model.

**Classifier B (ABI-level).** `abicompat <elf> <old-lib> <new-lib>`
(libabigail 2.5.0, `nix shell nixpkgs#libabigail`), designed for exactly
this app-vs-two-library-versions comparison. Cleared iff abicompat
reports no incompatibility (empty stdout, exit 0) against every lib the
consumer links.

**abicompat tripwire (hour-one check, as required by the boundary doc).**
Naive invocation crashes on every real Debian binary tested:

```
$ abicompat rsync libcrypto.so.3-old libcrypto.so.3-new
terminate called after throwing an instance of 'std::logic_error'
  what():  basic_string: construction from null is not valid
```

This reproduced on every consumer tried, including a locally-compiled,
unstripped, non-Debian test binary linked against the same Debian
libraries — ruling out "stripped binary" or "PIE" as the cause. It also
reproduced with `--fail-no-debug-info` (which is documented to bail out
cleanly rather than crash). Bisecting the flags: the crash is in
abicompat's _debug-info-directory auto-discovery_ path, which runs when
`--appd`/`--libd1`/`--libd2` are left unset and no debug info is present
(these Debian binaries are stripped with a `.gnu_debuglink` pointing at
an uninstalled `-dbgsym` package). Passing **any** existing directory
explicitly for all three of `--appd`, `--libd1`, `--libd2` (even an
empty one, so abicompat still finds zero debug info) avoids the crash
entirely and produces correct, non-crashing output identical in meaning
to "no debug info available." This is a one-line workaround, not a
hand-rolled analyzer — confirmed with a real ABI break (see Negative
controls below) that the tool's actual comparison logic runs normally
once the crash is avoided. `scripts/classify.py` applies this workaround
to every invocation and documents it inline.

**Negative controls (methodological, not part of the phase-1 sample).**
Because every real result below came back "cleared, no disagreement," we
verified both classifiers can actually detect a break before trusting
that result:

- Classifier A: re-run with the new-provider symbol set missing
  `libcrypto.so.3` (only `libssl.so.3` retained) reproducibly flags all 5
  attributed symbols on `rsync` as missing.
- Classifier B: re-run against a deliberately mismatched library pair
  (`libssl.so.3` where `libcrypto.so.3` was expected) reproducibly
  reports `incompat` with `ELF SONAME changed` and a real diff summary.

Both classifiers demonstrate real discriminative power; the 100%
clearable result is not a vacuous artifact of the harness.

## Results

- Reverse-dependency population: **838**
- Sample: **100** (seed 42, deterministic)
- No-ELF exclusions: **3** (`cl-plus-ssl` is arch:all Lisp source with no
  compiled binary; `libssl-dev` is a headers-only dev package; `pgloader`
  ships no ELF that itself links `libssl3` at the binary level — noted,
  not investigated further, since it's out of scope for what a rebind
  proxy needs to clear)
- Classified population (denominator): **97**
- ELF files classified: **173** (across 97 packages; up to 17 in one
  package, `tss2`)
- Classifier-A/B invocation pairs run: **248** (`abicompat` calls, one
  per (ELF, needed-lib) pair)

|                             | clearable          | frontier |
| --------------------------- | ------------------ | -------- |
| Classifier A (symbol-level) | **97 / 97 = 100%** | 0        |
| Classifier B (ABI-level)    | **97 / 97 = 100%** | 0        |

**A/B disagreement set: empty.** All 97 classified consumers agree
between the two classifiers (all cleared by both). Full per-consumer
detail, including every attributed symbol set and abicompat's raw
summary line, is in `phase1-results.csv`.

No false-clears are reported here because phase 2 (execution ground
truth) was not run — there is nothing to root-cause yet. That's the next
question, not this one.

## Phase-1 gate evaluation

Per the boundary document's staged gate:

- (a) clearable fraction tiny → NOT SELECTED (fraction is 100%, not
  tiny).
- (b) large A/B disagreement → NOT SELECTED (disagreement is 0/97).
- **(c) high clearable + checkers agree → SELECTED. Phase 2 is live and
  necessary.**

Row (c) is the one this run's numbers actually land on: a security-patch
point-release swap clears cheaply and both classifiers agree completely
on a real, unbiased 100-package sample. That is the necessary condition
for the whitepaper's headline claim, but not sufficient — it says
nothing yet about the false-clear rate (whether any of these 97 "cleared"
consumers would actually misbehave at runtime under the swap). That is
exactly what phase 2 is for.

## Recommendation (flagged for nrd's ratification)

Recommend proceeding to phase 2 as scoped in the boundary document:
since the A/B disagreement set is empty, phase 2's disagreement-targeted
sampling degenerates to just the "~10 randomly sampled B-cleared
consumers" false-clear floor — a small, cheap run. If those come back
clean, decision-table row 1 ("high clearable + ~0 false-clears at B")
is earned with real evidence behind it. The row selection itself, and
whether to spend the phase-2 budget now, is nrd's call — the evidence
only says phase 2 is the informative next step, not what it will find.

## Reproducing this run

```
cd tools/iface-proxy-study
bash scripts/fetch_libs_and_index.sh
python3 scripts/enumerate_consumers.py data/index/Packages.xz libssl3 100 42
bash scripts/fetch_consumers.sh 20241101T025324Z data/consumers-sample.csv
bash scripts/extract_and_find.sh
python3 scripts/classify.py
```

`data/index/Packages.xz` and the two `libssl3` `.deb`s are fetched
directly from the pinned URLs in `sources.lock` (not committed — see
`.gitignore` in this directory; they're multi-megabyte binary blobs
that snapshot.debian.org already pins immutably). Everything under
`data/consumers/` and `work/` is likewise regenerated by the scripts
above and not committed; `data/consumers-sample.csv`,
`data/consumers-all.txt`, `data/elf-manifest.tsv`, and
`phase1-results.csv` are committed since they're the actual evidence
artifacts.
