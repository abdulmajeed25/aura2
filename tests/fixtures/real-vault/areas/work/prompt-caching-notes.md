---
title: Anthropic Prompt Caching Notes
tags: [anthropic, prompt-caching, retrieval]
---

# Anthropic Prompt Caching Notes

Reference for the Phase 5 + Phase 7 swap where Aura starts calling
Claude with cached system prompts.

## Mechanics

- Cache lifetime: ~5 minutes from last read. Refreshing extends.
- Minimum cacheable block: 1024 tokens.
- Cost: write is ~1.25× a normal token; read is ~0.1×. Break-even
  after the second hit.
- Granularity: cache breakpoints set per content block in the request.

## Aura's caching strategy

1. **System prompt** (~3-4k tokens): vault overview + cortex state
   summary. Cached.
2. **Recent reflections** (~1-2k tokens): rolling window. Cached.
3. **Per-query** (~500 tokens): the actual question + retrieved chunks.
   Not cached.

Target: ≥70% hit ratio on the system prompt (per the spec metric).

## Crosslinks

- Where this gets wired: [[projects/aura/roadmap|Phase 5]].
- The retrieval pipeline that feeds it:
  [[areas/work/cognitive-architecture-notes|System 2 path]].
