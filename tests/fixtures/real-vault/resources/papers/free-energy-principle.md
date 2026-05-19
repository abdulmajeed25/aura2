---
title: Free Energy Principle — Karl Friston
tags: [research, free-energy, active-inference]
source: "Friston (2010). The Free Energy Principle: A Unified Brain Theory?"
---

# Free Energy Principle

Variational free energy as the quantity any self-organising system
minimises:

```
F = E_q[log q(θ) − log p(ỹ, θ)]
```

Reading `q(θ)` as the agent's belief about hidden states and
`p(ỹ, θ)` as the generative model that ties beliefs to observations,
`F` is an upper bound on surprise.

## Two ways to reduce F

1. **Perceptual inference**: change `q` to better match observations.
2. **Active inference**: act on the world to make observations match
   `q`.

## How Aura uses it

The cortex tracks `F` over the cognitive state. When `F` spikes:

- A reflection cycle fires.
- Hopfield retrieves the memories most likely to explain the surprise.
- Claude is called with those memories as context.
- The user sees the result in [[projects/aura/cognitive-core-design|the
  reflection feed]].

This is essentially active inference with the user as part of the
environment: the cortex acts (by surfacing a reflection) and observes
whether the user accepts it (which reduces predicted `F` further) or
dismisses it (which counts as evidence the prior was wrong).
