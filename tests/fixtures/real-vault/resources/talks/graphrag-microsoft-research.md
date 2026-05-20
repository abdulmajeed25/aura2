---
title: GraphRAG — Microsoft Research
tags: [research, graphrag, talks]
source: "MSR talk on hierarchical graph-based RAG"
---

# GraphRAG — Microsoft Research

Talk notes on the GraphRAG approach.

## The two-mode query

| Mode | Question shape | Mechanism |
|------|---------------|-----------|
| Local | "What did X say about Y?" | Top-K chunks via hybrid search |
| Global | "What are the themes in this corpus?" | Community summaries + map-reduce |

## The hierarchy

Leiden gives 4 levels at decreasing resolution. Each level's
communities get LLM-generated summaries; queries route by mode:

- Local → hybrid RRF over chunk embeddings.
- Global → top-K community summaries at the requested level.
- Drift → both, fused via a re-ranker.

## Aura's stand-in

The v3 build ships LPA + extractive summaries — see
[[projects/aura/roadmap|the roadmap]] for the swap. The seam is
`core::graph_rag::community_detector::detect_communities`. Replacing
LPA with Leiden is a single function swap.
