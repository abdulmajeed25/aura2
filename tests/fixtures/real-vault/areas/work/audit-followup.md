---
title: Audit Follow-up
tags: [audit, followup]
---

# Audit Follow-up

After the senior-QA audit of the v3 build (`TRUTH_AUDIT.md`):

## High severity
- **"Semantic search" is fake** with the shipped HashEmbedder. Lexical
  only. Closed by Phase 5.
- **GUI never launched**. Code-verified only. Closed by Phase 1 once
  we have a machine with a display.

## Medium severity
- **Path safety**: null bytes accepted; symlinks not canonicalised;
  `"."` accepted; `"C:\\Users"` accepted as a literal filename on Unix.
  Closed by Phase 1.
- **File watcher untested end-to-end**: every test calls `index_one`
  directly. Need a test that triggers a real `fs::write` and waits
  for the watcher debouncer. Closed by Phase 1.

## Low severity (closed in Phase 0)
- **`unwrap()`/`expect()` in production**: 2 occurrences fixed.
- **"98/98" headline**: actual is 97. Math error in commit messages.

## Open after Phase 0
- All medium + high items above.
- 9 stand-ins from [[projects/aura/roadmap|the roadmap]].

This note exists in the fixture so a future cortex run could surface
"the audit findings cluster" as a community.
