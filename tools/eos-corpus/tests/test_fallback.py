"""Tests for fallback.py: tier-2 heuristic routing and resolve_duration."""

import pytest
from eos_corpus.fallback import resolve_duration, tier2_duration, _DEFAULT_DURATION


class TestTier2Duration:
    def test_named_package_hydra_measured(self):
        # Hydra-measured packages in _NAMED take tier-2a priority.
        # Values are jittered, but without a drv_id they return the exact midpoint.
        assert tier2_duration("ripgrep-14.1.1") == 68.0
        assert tier2_duration("curl-8.12.1") == 33.0
        assert tier2_duration("jq-1.8.1") == 20.0
        assert tier2_duration("linux-6.12.0") == 1477.0

    def test_named_package_community(self):
        assert tier2_duration("gcc-14.2.0") == 1800.0       # named table > pattern
        assert tier2_duration("openssl-3.3.0") == 90.0
        assert tier2_duration("zlib-1.3.1") == 8.0
        assert tier2_duration("glibc-2.40") == 300.0

    def test_jitter_applied_when_drv_id_given(self):
        # With a drv_id, the result is no longer the exact midpoint.
        without = tier2_duration("ripgrep-14.1.1")
        with_id = tier2_duration("ripgrep-14.1.1", drv_id="abc123def456")
        assert without == 68.0
        # Should differ (jitter applied) and be positive
        assert with_id > 0
        # Log-normal with σ=0.4 keeps result within [0.1×, 10×] of midpoint
        assert 6.8 < with_id < 680.0

    def test_jitter_deterministic(self):
        # Same drv_id always yields same result.
        a = tier2_duration("curl-8.12.1", drv_id="aabbccdd1122")
        b = tier2_duration("curl-8.12.1", drv_id="aabbccdd1122")
        assert a == b

    def test_jitter_varies_by_drv_id(self):
        # Different drv_ids should produce different results.
        a = tier2_duration("unknown-pkg-1.0", input_count=5, drv_id="00000000aaaa")
        b = tier2_duration("unknown-pkg-1.0", input_count=5, drv_id="ffffffff1111")
        assert a != b

    def test_source_fetch_no_jitter(self):
        # Structural role patterns (tier-2b) are NOT jittered.
        assert tier2_duration("source") == 6.0
        assert tier2_duration("fetchurl") == 6.0
        assert tier2_duration("source", drv_id="abc123def456") == 6.0

    def test_compiler_pattern_still_catches_unlisted(self):
        # gcc is now in _NAMED, but clang-unwrapped (non-versioned) still hits pattern.
        assert tier2_duration("clang-unwrapped") == 510.0

    def test_doc_man(self):
        for name in ["ripgrep-doc", "man-pages", "gtk4-docs"]:
            assert tier2_duration(name) == 17.0, f"failed for {name}"

    def test_hook_wrapper_setup(self):
        for name in ["setup-hook", "python-wrapper", "wrap-gapps-hook"]:
            assert tier2_duration(name) == 3.0, f"failed for {name}"

    def test_heft_model_no_drv_id(self):
        # Unknown package with input_count falls through to heft tier.
        assert tier2_duration("some-unknown-pkg", input_count=0) == 10.0
        assert tier2_duration("some-unknown-pkg", input_count=5) == 40.0
        assert tier2_duration("some-unknown-pkg", input_count=15) == 80.0
        assert tier2_duration("some-unknown-pkg", input_count=40) == 160.0
        assert tier2_duration("some-unknown-pkg", input_count=100) == 300.0

    def test_default_bucket_when_no_input_count(self):
        # Truly unknown package with no input_count falls to _DEFAULT_DURATION.
        assert tier2_duration("totally-unknown-xyzzy") == _DEFAULT_DURATION


class TestResolveDuration:
    def test_tier1_used_when_positive(self):
        dur, measured = resolve_duration("ripgrep-14.1.1", tier1=42.5)
        assert dur == 42.5
        assert measured is True

    def test_tier2_when_tier1_none(self):
        # ripgrep is in _NAMED — returns 68.0 without drv_id
        dur, measured = resolve_duration("ripgrep-14.1.1", tier1=None)
        assert dur == 68.0
        assert measured is False

    def test_tier2_when_tier1_zero(self):
        dur, measured = resolve_duration("source", tier1=0)
        assert dur == 6.0
        assert measured is False

    def test_tier2_when_tier1_negative(self):
        dur, measured = resolve_duration("source", tier1=-1.0)
        assert dur == 6.0
        assert measured is False

    def test_compiler_gets_named_table_value(self):
        # gcc now comes from _NAMED (1800s) not _PATTERNS (510s)
        dur, measured = resolve_duration("gcc-14.2.0", tier1=None)
        assert dur == 1800.0
        assert measured is False

    def test_drv_id_propagated_to_jitter(self):
        # With drv_id, result should differ from the exact midpoint.
        dur, measured = resolve_duration(
            "ripgrep-14.1.1", tier1=None, drv_id="deadbeef1234"
        )
        assert measured is False
        assert dur > 0
        # Should not be the raw 68.0 midpoint (jitter applied)
        assert dur != 68.0
