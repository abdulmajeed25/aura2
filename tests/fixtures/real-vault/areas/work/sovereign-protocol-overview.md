---
title: Sovereign Protocol — Overview
tags: [sovereign, contracts, shariah]
status: draft
---

# Sovereign Protocol — Overview

A Shariah-compliant escrow + settlement primitive that I want to vendor
across both Sovereign and [[projects/taminat/billing-engine|Taminat]].

## Escrow rule

Funds split as `(provider_share, platform_share, regulator_share)` with
invariant `provider_share + platform_share + regulator_share == 100`.
^split-rule

The settlement is finalised when:
1. Both counterparties acknowledge delivery.
2. The regulator's audit hook returns SAT.
3. The time-lock has elapsed.

## Crosslinks

- The fee-split algebra is mirrored in [[projects/taminat/billing-engine#^settlement-split]].
- The audit hook talks to the contracts described in [[projects/taminat/shariah-compliance-checklist]].

## Algebra (the part the cortex should notice)

`(a, b, c)` with `a + b + c = 100`. Two contracts with the same shape
should land at the same point in the cortex's attractor space — and
when they don't, that's a contradiction worth surfacing as a reflection.
