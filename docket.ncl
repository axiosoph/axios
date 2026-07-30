# Genre hierarchy for the axios corpus (docket).
#
# Each genre holds exactly one claim kind and may cite but never restate.
# Patterns MUST be disjoint: a path matching more than one genre is a
# configuration error, not a precedence question.
#
# `quadrant` names which of the four documentation quadrants a genre serves.
# The genre taxonomy above is specific to this repository and does not travel;
# the quadrant is universal, so it is what makes a stability reading comparable
# across corpora and what turns "which quadrant carries no claims" into a
# computed answer.
#
# It also makes the ADR row's empty `kinds` DERIVED rather than chosen: a claim
# is a checkable assertion and explanation's job is rationale, so explanation
# carries no claims. The loader enforces that, so the row cannot drift from the
# reason for it.
#
# No `www/content/**` row, deliberately. Its `reference/`, `models/` and
# `architecture/` subtrees carry only `_index.md` as tracked files - everything
# beneath them is generated site output. A genre there would point the register
# at build artifacts, where a claim block means nothing because the output is
# regenerated. If rows are ever wanted, they are narrow and per-subtree over the
# authored prose (`explanation/`, `how-to/`), never a blanket glob over the
# mirror root.
{
  genres = [
    { path = "docs/specs/**",        kinds = ["constraint"],  quadrant = "reference" },
    { path = "docs/models/**",       kinds = ["invariant"],   quadrant = "reference" },
    { path = "docs/architecture/**", kinds = ["requirement"], quadrant = "reference" },
    { path = "docs/adr/**",          kinds = [],              quadrant = "explanation" },
  ],
}
