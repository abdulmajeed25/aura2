---
title: Aura — Decision Log
tags: [aura, decisions, adr]
---

# Aura — Decision Log

ADR-style record of architectural decisions.

## ADR-001 — Canvas 2D over Pixi.js (Phase 4)

**Date**: 2026-03-12
**Decision**: Render the Graph view with raw Canvas 2D, not Pixi.js.
**Reason**: 2k-node target hits 60 FPS on Canvas; Pixi adds ~200kB
bundle weight + a React adapter risk.
**Reconsider when**: target moves to 10k+ nodes per Phase 4 (v5.0) stretch goal.

## ADR-002 — libsql over rusqlite (Phase 1)

**Date**: 2026-02-20
**Decision**: Use libsql (Turso's SQLite fork) for the local DB.
**Reason**: Modern API, easier futures, native async, FTS5 included.

## ADR-003 — EMA stand-in for SSM (Phase 8)

**Date**: 2026-04-30
**Decision**: Ship a 384-dim EMA hidden state with α=0.82 as the
stand-in for Mamba ONNX.
**Reason**: HF model download blocked in build sandbox; EMA preserves
fixed-RAM streaming property that's the user-visible bit.
**Disclosed in**: [[projects/aura/roadmap|the roadmap]] +
`docs/STAND_IN_REGISTRY.md` entry #5.

## ADR-004 — Sidecars allowed (Phase 14/15)

**Date**: 2026-05-18
**Decision**: Python sidecars (Letta, LightRAG, LLMLingua-2) are
allowed. Aura ships a tray-icon indicator showing which sidecars
are running.
**Reason**: Rewriting Letta in Rust is 4 weeks; sidecar is hours.
The cost is the user needs Python available.
