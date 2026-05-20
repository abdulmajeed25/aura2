---
title: LightRAG vs GraphRAG — Comparison
tags: [research, lightrag, graphrag]
---

# LightRAG vs GraphRAG — Comparison

| Aspect | Microsoft GraphRAG | LightRAG (HKU) |
|--------|--------------------|----------------|
| Granularity | 4-level Leiden hierarchy | Dual-level: entity + theme |
| Ingest cost | High (LLM summary per community per level) | Lower (entity extraction + dedup) |
| Query latency | Higher for global mode | Lower; designed for streaming |
| License | MIT | MIT |
| Sidecar in Aura | optional | preferred default per spec |

The Phase 7 plan keeps both options. `lightrag.enabled = true` in
config spawns the LightRAG sidecar; `false` uses the in-Rust Leiden
([[resources/talks/graphrag-microsoft-research|GraphRAG]]-style)
+ extractive summary path.

## Crosslinks

- Aura's GraphRAG stand-in:
  [[projects/aura/roadmap|see Phase 7]].
- Why Aura needs both retrieval and a cortex:
  [[areas/work/cognitive-architecture-notes|System 1 vs System 2]].
