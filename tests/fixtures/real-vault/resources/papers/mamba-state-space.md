---
title: Mamba — Selective State Space Models
tags: [research, ssm, mamba]
source: "Gu & Dao (2024). Mamba: Linear-Time Sequence Modeling"
---

# Mamba — Selective State Space Models

Reading notes for the Phase 8 SSM swap.

## The selective scan

Where a vanilla SSM has data-independent `A, B, C` matrices, Mamba
makes them functions of the input. The state update becomes:

```
B(t) = f_B(x(t))
C(t) = f_C(x(t))
Δ(t) = softplus(f_Δ(x(t)))
A_bar = exp(Δ ⊗ A)
B_bar = (A_bar - I) ⋅ A^{-1} ⋅ B
h(t)  = A_bar(t) · h(t-1) + B_bar(t) · x(t)
y(t)  = C(t) · h(t)
```

The hidden state `h` stays fixed-size; the *transition* changes per
token. That's the selectivity that makes Mamba competitive with
attention without `O(N²)` cost.

## Why it matters for Aura

Phase 8 wants a streaming reasoning backbone whose RAM doesn't grow
with conversation length. EMA (the v3 stand-in) gives fixed RAM but
no input-dependent selectivity. Mamba INT8 at ~130MB ONNX would give
us both. If the ONNX conversion fails, Phi-3-mini is the path-C
fallback per [[projects/aura/roadmap|the roadmap]].

## Open questions

- Does the streaming property survive INT8 quantisation? See
  [[daily/2026/05/2026-05-18|today's daily]] for the experiment plan.
- Can we run Mamba on the same `cognitive_state ∈ ℝ^512` space the
  cortex uses, or does it need its own `d_model = 768`?
