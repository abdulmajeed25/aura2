---
title: Cognitive Architecture Notes
tags: [cognition, neuroscience, architecture]
created: 2026-04-12
---

# Cognitive Architecture Notes

Working notes on dual-process architectures (System 1 fast / System 2
deliberative) and how to map them onto a software cortex.

## The two systems

System 1 corresponds to continuous, autonomous loops (perception,
emotion, motor priors). In Aura, this is the perpetual loop in
`cognition::perpetual_loop`. It runs at `dt = 0.01s`, integrates
Wilson-Cowan attractor dynamics, and never sleeps.
^system1-loop

System 2 is the deliberate, language-driven path. In Aura, it's the
Claude-mediated reasoning layer that the user invokes explicitly.
It runs at *seconds-per-thought* cadence, not milliseconds.

## Why a Liquid State Machine

LSMs let us inject sparse, time-varying inputs into a high-dimensional
reservoir and read out features without retraining the reservoir. That
matches "user types something, cortex integrates it, downstream layers
read out" very well. See [[resources/papers/vsa-foundations]] for the
algebraic side.

## Open questions

- Does a real Hopfield retrieval beat HDC bundle-and-unbind for the
  same recall task? Pending: benchmark on the [[projects/aura/cognitive-core-design]] eval set.
- Hamiltonian fusion: does Claude's "intellectual momentum" injected as
  `p` actually reduce free energy? Phase 12 telemetry will tell.
