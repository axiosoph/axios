#!/usr/bin/env python3
"""Axios doctrine-trap lints.

Guards the codebase against silent resurrection of concepts the doctrine has
retired (ADR-0006 execution model, the AGENTS.md CAUTION glossary) and against
identity types leaking a wire representation. Every lint targets CODE
IDENTIFIERS (enum arms, struct/field/type names) or exact banned phrases, never
English prose that legitimately narrates the history of a retired concept.

Vehicle rationale: a comment/structure-aware Python scanner, not clippy/dylint
(custom-lint infra is disproportionate for line-level doctrine tokens) and not
an extension of the compliance tracker (which measures spec-constraint coverage,
an orthogonal axis — folding these in would complect two concerns).

Pre-existing legacy sites are WAIVED, never exempted by narrowing a lint's
scope: see ``waivers.toml``. The waiver list may only shrink — a waiver that no
longer matches a real violation is itself a failure (remove it).

Usage:
    doctrine_lint.py            scan the repository; exit 1 on any violation
    doctrine_lint.py --self-test   property-test the lints against synthetic
                                   violation and clean inputs (no repo scan)

Environment:
    LINT_DIFF_BASE   git ref the diff-scoped glossary lint diffs against
                     (default: master). If the ref is absent the glossary lint
                     is skipped with a warning rather than failing.
"""

from __future__ import annotations

import os
import re
import subprocess
import sys
import tomllib
from dataclasses import dataclass

SELF_DIR = os.path.dirname(os.path.abspath(__file__))
REPO_ROOT = os.path.dirname(os.path.dirname(SELF_DIR))
WAIVERS_PATH = os.path.join(SELF_DIR, "waivers.toml")

# Files that must be allowed to NAME the forbidden tokens because they define or
# forbid them. Not a silent scope-narrowing: these are the doctrine source and
# the lint machinery itself, recorded here in the open.
DOCTRINE_SOURCE = "AGENTS.md"  # root glossary + the .scratch/.sketches rule
LINT_DIR_PREFIX = "tools/lints/"  # the lint scripts + waiver inventory


@dataclass(frozen=True)
class Violation:
    lint: str
    path: str
    line: int
    text: str


# --------------------------------------------------------------------------- #
# Rust comment handling
# --------------------------------------------------------------------------- #
def rust_code_part(line: str) -> str:
    """Return the code portion of a Rust line, dropping any ``//`` comment.

    A ``//`` preceded by ``:`` (as in ``https://``) is treated as code, not a
    comment. Block comments (``/* */``) are not handled — a documented, accepted
    limitation given none appear on the linted surface.
    """
    i = 0
    n = len(line)
    while i < n - 1:
        if line[i] == "/" and line[i + 1] == "/":
            if i > 0 and line[i - 1] == ":":
                i += 1
                continue
            return line[:i]
        i += 1
    return line


# --------------------------------------------------------------------------- #
# Per-line / per-file checks (pure — operate on supplied text, not the FS)
# --------------------------------------------------------------------------- #
_IDENTIFIER_LINTS = {
    # lint name -> (compiled whole-word pattern, file suffix)
    "needs-evaluation": re.compile(r"\bNeedsEvaluation\b"),
    "evaluator": re.compile(r"\bevaluator\b", re.IGNORECASE),
    "compose-config": re.compile(r"\bComposeConfig\b"),
}


def scan_rust_identifiers(path: str, lines: list[str]) -> list[Violation]:
    """Flag retired doctrine tokens appearing as Rust CODE identifiers.

    Comment lines and trailing comments are stripped first, so prose narration
    of a retired concept ("NeedsEvaluation names a stage that no longer exists")
    is exempt by construction.
    """
    out: list[Violation] = []
    for n, raw in enumerate(lines, start=1):
        code = rust_code_part(raw)
        if not code.strip():
            continue
        for lint, pat in _IDENTIFIER_LINTS.items():
            if pat.search(code):
                out.append(Violation(lint, path, n, raw.rstrip("\n")))
    return out


_NEWTYPE_RE = re.compile(
    r"^\s*pub\s+struct\s+(\w+)\s*\(\s*(?:pub\s+)?(?:\[u8;\s*\d+\]|Digest)\s*\)\s*;"
)


def scan_serde_identity(path: str, lines: list[str]) -> list[Violation]:
    """Flag identity newtypes (``ReqDigest``/``ActionId`` class) deriving serde.

    An identity newtype is a single-field tuple struct wrapping ``[u8; N]`` or
    ``Digest`` — an opaque content-address handle, not a wire type. If serde is
    derived on it, an implicit serialized form leaks; forbidden.
    """
    out: list[Violation] = []
    for i, raw in enumerate(lines):
        if not _NEWTYPE_RE.match(raw):
            continue
        # Walk the contiguous attribute block immediately above the struct.
        attrs: list[str] = []
        j = i - 1
        while j >= 0 and lines[j].lstrip().startswith("#["):
            attrs.append(lines[j])
            j -= 1
        blob = " ".join(attrs)
        if "Serialize" in blob or "Deserialize" in blob:
            out.append(Violation("serde-identity", path, i + 1, raw.rstrip("\n")))
    return out


def scan_scratch(path: str, lines: list[str]) -> list[Violation]:
    """Flag committed DOC references to the git-ignored ``.scratch``/``.sketches``.

    Scoped to documentation (``*.md``) per c3 ("committed docs"): a committed
    doc must read whole to a stranger holding only the repository. Deliberately
    NOT applied to executable code — a tool routing its own ephemeral output into
    ``.scratch/`` (e.g. eos-sweep defaults) is correct three-stores behavior, and
    ``.gitignore`` legitimately names the paths to ignore them.
    """
    pat = re.compile(r"\.scratch\b|\.sketches\b")
    out: list[Violation] = []
    for n, raw in enumerate(lines, start=1):
        if pat.search(raw):
            out.append(Violation("scratch-reference", path, n, raw.rstrip("\n")))
    return out


# --------------------------------------------------------------------------- #
# Glossary lint (diff-scoped) — see justify_glossary_scope() for the rationale.
# --------------------------------------------------------------------------- #
def glossary_added_line(
    path: str, content: str, cfg: dict
) -> list[Violation]:
    """Flag a NEWLY ADDED line that resurrects a deprecated glossary alias.

    Scoped three ways to avoid the false-positive storm that a full-surface
    grep of these common words would produce (``derivation`` appears ~40x as a
    legitimate ``nix_compat`` identifier; ``genesis`` is git vocabulary;
    ``root`` names closure roots): (1) new Rust TYPE declarations named after an
    ambiguous alias, (2) the unambiguous token ``Blake3Hash`` (zero legitimate
    uses), (3) exact banned phrases.
    """
    if path == DOCTRINE_SOURCE or path.startswith(LINT_DIR_PREFIX):
        return []
    out: list[Violation] = []
    decl_terms = "|".join(re.escape(t) for t in cfg["decl_terms"])
    decl_re = re.compile(
        r"\b(?:struct|enum|type|trait|union)\s+(?:" + decl_terms + r")\b"
    )
    if path.endswith(".rs"):
        code = rust_code_part(content)
        if decl_re.search(code):
            out.append(Violation("glossary", path, 0, content.rstrip("\n")))
            return out
    for tok in cfg["tokens"]:
        if re.search(r"\b" + re.escape(tok) + r"\b", content):
            out.append(Violation("glossary", path, 0, content.rstrip("\n")))
            return out
    for phrase in cfg["phrases"]:
        if phrase in content:
            out.append(Violation("glossary", path, 0, content.rstrip("\n")))
            return out
    return out


def scan_glossary_diff(cfg: dict, base: str) -> tuple[list[Violation], str | None]:
    """Diff base..worktree and flag deprecated-alias resurrection on added lines.

    Returns (violations, warning). If ``base`` is unresolvable the glossary lint
    is SKIPPED (warning returned, no violation) so the gate remains runnable in
    any checkout.
    """
    if (
        subprocess.run(
            ["git", "rev-parse", "--verify", "--quiet", base + "^{commit}"],
            cwd=REPO_ROOT,
            capture_output=True,
        ).returncode
        != 0
    ):
        return [], f"glossary lint skipped: diff base '{base}' not found"
    diff = subprocess.run(
        ["git", "diff", "--unified=0", "--no-color", base, "--", "*.rs", "*.md", "*.toml"],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    out: list[Violation] = []
    cur = None
    for line in diff.splitlines():
        if line.startswith("+++ "):
            p = line[4:]
            cur = p[2:] if p.startswith("b/") else p
        elif line.startswith("+") and not line.startswith("+++"):
            if cur is not None:
                out.extend(glossary_added_line(cur, line[1:], cfg))
    return out, None


# --------------------------------------------------------------------------- #
# Repository scan
# --------------------------------------------------------------------------- #
def tracked_files() -> list[str]:
    res = subprocess.run(
        ["git", "ls-files"], cwd=REPO_ROOT, capture_output=True, text=True, check=True
    )
    return res.stdout.splitlines()


def read_lines(path: str) -> list[str] | None:
    try:
        with open(os.path.join(REPO_ROOT, path), "r", encoding="utf-8") as f:
            return f.readlines()
    except (UnicodeDecodeError, FileNotFoundError, IsADirectoryError):
        return None


def scan_repo(cfg: dict) -> tuple[list[Violation], list[str]]:
    violations: list[Violation] = []
    warnings: list[str] = []
    for path in tracked_files():
        lines = read_lines(path)
        if lines is None:
            continue
        if path.endswith(".rs"):
            violations += scan_rust_identifiers(path, lines)
            violations += scan_serde_identity(path, lines)
        # scratch check: documentation only (*.md), excluding the doctrine source
        # (AGENTS.md), which names the paths to forbid them.
        if path.endswith(".md") and path != DOCTRINE_SOURCE:
            violations += scan_scratch(path, lines)

    base = os.environ.get("LINT_DIFF_BASE", "master")
    gloss, warn = scan_glossary_diff(cfg["glossary"], base)
    violations += gloss
    if warn:
        warnings.append(warn)
    return violations, warnings


# --------------------------------------------------------------------------- #
# Waivers
# --------------------------------------------------------------------------- #
def load_config() -> dict:
    with open(WAIVERS_PATH, "rb") as f:
        return tomllib.load(f)


def apply_waivers(
    violations: list[Violation], waivers: list[dict]
) -> tuple[list[Violation], list[dict]]:
    """Return (unwaived violations, stale waivers).

    A waiver matches a violation when lint and path agree and the waiver's
    ``symbol`` occurs in the violation text. A waiver matching nothing is stale.
    """
    remaining: list[Violation] = []
    matched: set[int] = set()
    for v in violations:
        hit = False
        for idx, w in enumerate(waivers):
            if (
                w["lint"] == v.lint
                and w["path"] == v.path
                and w["symbol"] in v.text
            ):
                matched.add(idx)
                hit = True
        if not hit:
            remaining.append(v)
    stale = [w for idx, w in enumerate(waivers) if idx not in matched]
    return remaining, stale


# --------------------------------------------------------------------------- #
# Self-test: property-test each lint against synthetic violation + clean input.
# --------------------------------------------------------------------------- #
def self_test(cfg: dict) -> int:
    failures: list[str] = []

    def expect(cond: bool, msg: str) -> None:
        if not cond:
            failures.append(msg)

    # needs-evaluation / evaluator / compose-config: fire on code, exempt prose.
    fire = scan_rust_identifiers(
        "x.rs",
        [
            "    NeedsEvaluation(AtomRef<D>),",
            "    pub evaluator: Executor,",
            "    pub compose: ComposeConfig,",
        ],
    )
    kinds = {v.lint for v in fire}
    expect("needs-evaluation" in kinds, "needs-evaluation must fire on enum arm")
    expect("evaluator" in kinds, "evaluator must fire on code identifier")
    expect("compose-config" in kinds, "compose-config must fire on field type")

    clean = scan_rust_identifiers(
        "x.rs",
        [
            "//!   NeedsEvaluation names a stage that no longer exists.",
            "/// universal machine runner, not an evaluator — nothing interprets",
            "        // Cache miss -> NeedsEvaluation",
            "    let request = EvalRequest::new();",  # 'evaluator' != 'EvalRequest'
        ],
    )
    expect(not clean, f"prose narration must be exempt, got {clean}")

    # serde-identity: fire when serde is derived on an identity newtype.
    sfire = scan_serde_identity(
        "x.rs",
        ["#[derive(Serialize, Deserialize)]", "pub struct FakeId(pub [u8; 32]);"],
    )
    expect(len(sfire) == 1, f"serde-identity must fire on serde newtype, got {sfire}")
    sclean = scan_serde_identity(
        "x.rs",
        [
            "#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]",
            "pub struct ReqDigest(pub [u8; 32]);",
            "#[derive(Clone, Copy)]",
            "pub struct CompositionRoot(pub Digest);",
        ],
    )
    expect(not sclean, f"clean identity newtypes must pass, got {sclean}")

    # scratch-reference.
    expect(
        bool(scan_scratch("docs/x.md", ["see .scratch/notes for detail"])),
        "scratch-reference must fire on committed .scratch mention",
    )
    expect(
        not scan_scratch("docs/x.md", ["the deliverable is self-contained"]),
        "scratch-reference must not fire on clean prose",
    )

    # glossary (diff-scoped added lines).
    g = cfg["glossary"]
    for content in (
        "pub struct Derivation {",
        "    let h: Blake3Hash = compute();",
    ):
        expect(
            bool(glossary_added_line("x.rs", content, g)),
            f"glossary must fire on resurrected alias: {content!r}",
        )
    expect(
        bool(glossary_added_line("docs/x.md", "the hashed atom id is deprecated-shaped", g)),
        "glossary must fire on banned phrase",
    )
    for path, content in (
        ("eos/eos-snix/src/build.rs", "use nix_compat::derivation::Derivation;"),
        ("atom/atom-git/src/x.rs", "    let root = repo.head_commit();"),
        ("htc/htc-exec/src/lib.rs", "pub struct CompositionRoot(pub Digest);"),
        ("atom/atom-git/src/x.rs", "    // create a genesis commit for the test"),
        (DOCTRINE_SOURCE, "| Digest | ... | AtomDigest, Blake3Hash |"),
        ("tools/lints/waivers.toml", 'tokens = ["Blake3Hash"]'),
    ):
        expect(
            not glossary_added_line(path, content, g),
            f"glossary must NOT fire (false positive) on {path}: {content!r}",
        )

    if failures:
        print("SELF-TEST FAILURES:")
        for m in failures:
            print(f"  - {m}")
        return 1
    print(f"self-test PASS ({6} lint classes exercised, violation + clean cases)")
    return 0


# --------------------------------------------------------------------------- #
# Entry point
# --------------------------------------------------------------------------- #
def main(argv: list[str]) -> int:
    cfg = load_config()
    if "--self-test" in argv:
        return self_test(cfg)

    violations, warnings = scan_repo(cfg)
    remaining, stale = apply_waivers(violations, cfg.get("waiver", []))

    for w in warnings:
        print(f"WARNING: {w}")

    if not remaining and not stale:
        n_waived = len(violations) - len(remaining)
        print(
            f"doctrine-lint PASS: 0 unwaived violations "
            f"({n_waived} pre-existing site(s) waived, {len(cfg.get('waiver', []))} waiver rows)"
        )
        return 0

    if remaining:
        print(f"doctrine-lint FAIL: {len(remaining)} unwaived violation(s):")
        for v in sorted(remaining, key=lambda x: (x.lint, x.path, x.line)):
            loc = f"{v.path}:{v.line}" if v.line else v.path
            print(f"  [{v.lint}] {loc}: {v.text.strip()}")
    if stale:
        print(f"doctrine-lint FAIL: {len(stale)} stale waiver(s) (list may only shrink — remove):")
        for w in stale:
            print(f"  [{w['lint']}] {w['path']} :: {w['symbol']}")
    return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
