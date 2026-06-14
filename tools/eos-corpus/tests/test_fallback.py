"""Tests for fallback.py: tier-2 heuristic routing and resolve_duration."""

import pytest
from eos_corpus.fallback import resolve_duration, tier2_duration, _DEFAULT_DURATION


class TestTier2Duration:
    def test_compiler_names(self):
        for name in ["gcc-14.2.0", "clang-18-unwrapped", "rustc-1.80.0", "llvm-18"]:
            assert tier2_duration(name) == 510.0, f"failed for {name}"

    def test_source_fetch(self):
        for name in ["source", "fetchurl", "ripgrep-source", "curl-src"]:
            assert tier2_duration(name) == 6.0, f"failed for {name}"

    def test_doc_man(self):
        for name in ["ripgrep-doc", "man-pages", "gtk4-docs"]:
            assert tier2_duration(name) == 17.0, f"failed for {name}"

    def test_hook_wrapper_setup(self):
        for name in ["setup-hook", "python-wrapper", "wrap-gapps-hook"]:
            assert tier2_duration(name) == 3.0, f"failed for {name}"

    def test_default_bucket(self):
        for name in ["ripgrep-14.1.1", "openssl-3.3.0", "glibc-2.40"]:
            assert tier2_duration(name) == _DEFAULT_DURATION, f"failed for {name}"

    def test_compiler_priority_over_default(self):
        # gcc should hit compiler pattern, not default
        assert tier2_duration("gcc-wrapper-14.2.0") == 510.0


class TestResolveDuration:
    def test_tier1_used_when_positive(self):
        dur, measured = resolve_duration("ripgrep-14.1.1", tier1=42.5)
        assert dur == 42.5
        assert measured is True

    def test_tier2_when_tier1_none(self):
        dur, measured = resolve_duration("ripgrep-14.1.1", tier1=None)
        assert dur == _DEFAULT_DURATION
        assert measured is False

    def test_tier2_when_tier1_zero(self):
        # buildstatus=cached yields starttime==stoptime → duration=0
        dur, measured = resolve_duration("source", tier1=0)
        assert dur == 6.0
        assert measured is False

    def test_tier2_when_tier1_negative(self):
        dur, measured = resolve_duration("source", tier1=-1.0)
        assert dur == 6.0
        assert measured is False

    def test_compiler_gets_heuristic_without_tier1(self):
        dur, measured = resolve_duration("gcc-14.2.0", tier1=None)
        assert dur == 510.0
        assert measured is False
