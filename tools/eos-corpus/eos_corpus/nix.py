"""Nix subprocess helpers: derivation show, git checkout, HEAD restore."""

from __future__ import annotations

import fcntl
import json
import subprocess
from contextlib import contextmanager
from pathlib import Path
from typing import Any, Dict


def derivation_show(nixpkgs_path: str, attr: str) -> Dict[str, Any]:
    """Run ``nix derivation show --recursive`` for attr in nixpkgs at PATH.

    Returns the parsed JSON object (drv-path → descriptor).
    Does not modify the nixpkgs checkout; caller manages HEAD.
    """
    expr = f"path:{nixpkgs_path}#{attr}"
    result = subprocess.run(
        ["nix", "derivation", "show", "--recursive", expr],
        capture_output=True,
        text=True,
        check=True,
    )
    return json.loads(result.stdout)


@contextmanager
def at_commit(repo_path: str, commit: str):
    """Context manager: checkout ``commit`` in ``repo_path``, restore HEAD on exit.

    Holds an exclusive advisory flock on .eos-corpus.lock inside the repo
    for the duration of the checkout so concurrent callers (e.g. two
    simultaneous eos-corpus subcommands) cannot corrupt one another's HEAD.
    """
    lock_path = Path(repo_path) / ".eos-corpus.lock"
    with open(lock_path, "w") as lock_f:
        fcntl.flock(lock_f, fcntl.LOCK_EX)
        orig = subprocess.run(
            ["git", "-C", repo_path, "rev-parse", "HEAD"],
            capture_output=True, text=True, check=True,
        ).stdout.strip()
        subprocess.run(
            ["git", "-C", repo_path, "checkout", "--quiet", commit],
            check=True,
        )
        try:
            yield
        finally:
            subprocess.run(
                ["git", "-C", repo_path, "checkout", "--quiet", orig],
                check=True,
            )
            fcntl.flock(lock_f, fcntl.LOCK_UN)


def merge_commits_from_branch(repo_path: str, branch: str = "staging-next") -> list[str]:
    """Return SHAs of all merge commits that brought ``branch`` into master.

    Looks for merge commit messages containing the branch name.
    """
    result = subprocess.run(
        [
            "git", "-C", repo_path, "log",
            "--merges", "--oneline", "--format=%H %s",
            "master",
        ],
        capture_output=True, text=True, check=True,
    )
    commits = []
    for line in result.stdout.splitlines():
        sha, _, subject = line.partition(" ")
        if branch in subject:
            commits.append(sha)
    return commits
