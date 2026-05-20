---
title: Idea — FeeSplit abstraction
tags: [inbox, idea]
status: raw
---

# Idea — FeeSplit abstraction

`(provider_share, platform_share, regulator_share)` with invariant
`sum == 100` appears in both:
- [[areas/work/sovereign-protocol-overview#^split-rule|Sovereign escrow]]
- [[projects/taminat/billing-engine#^settlement-split|Taminat settlement]]

Same algebra. Extract a shared `FeeSplit` type in a vendored crate
both projects depend on. Single source of truth for Shariah audits.

This is exactly the kind of pattern the cortex should surface as a
reflection automatically — same algebraic structure showing up in two
contexts means the user's knowledge graph has a hidden equivalence.
