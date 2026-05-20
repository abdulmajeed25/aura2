---
title: VSA Foundations
tags: [research, vsa, hdc]
source: "Plate, T. (1995). Holographic Reduced Representations"
---

# VSA Foundations

Reading notes on Vector Symbolic Architectures.

## The three core operations

1. **Bundling**: superposition of a set of items into one vector.
   In bipolar HDC: `sign(sum(v_i))`. Preserves similarity to each
   component above chance.
2. **Binding**: associating one vector with another to form a
   role-filler pair. In bipolar HDC: element-wise product (Hadamard).
   Self-inverse: `bind(A, A) = +1`.
3. **Permutation**: cyclic shift, used to encode order. Makes
   `permute(A, 1)` ~orthogonal to `A`.

## The capacity bound (Plate)

For dimension `D` and `k` independent role-filler pairs bundled, the
noise on retrieval scales like `1/√D`. Practical capacity ≈
`D / (4 log K)`.

For `D = 10_000` and an entropy of 10 bits per pair this gives ~250
independent pairs before retrieval breaks down.

## Connection to Aura

The HDC engine in [[projects/aura/cognitive-core-design|the cortex
design]] uses bipolar binding and a permutation of 1 for outgoing
edges, 2 for incoming. The asymmetry prevents `A→B` and `B→A`
collapsing to the same neighbourhood signature. See
[[areas/work/cognitive-architecture-notes]] for the protocol
implementation.
