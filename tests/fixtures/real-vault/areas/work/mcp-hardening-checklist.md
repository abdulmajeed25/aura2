---
title: MCP Hardening Checklist
tags: [mcp, security, claw-chain]
---

# MCP Hardening Checklist

Distilled from the OpenClaw "Claw Chain" CVEs (44112, 44113, 44115, 44118).

| Vulnerability class | Aura mitigation |
|---------------------|----------------|
| TOCTOU file ops | Atomic `open + fstat` only; never resolve path twice. |
| Symlink escape | `canonicalize` then verify still under vault root. |
| Heredoc env expansion | No shell heredocs anywhere. Always explicit `argv`. |
| Client-controlled ownership | Owner derived **only** from authenticated token. |
| Stale bearer tokens | Token rotated on every restart. |
| Null-byte paths | Reject at API boundary. |
| Localhost ≠ trusted | All endpoints require auth even on 127.0.0.1. |
| Unbounded request bodies | 10 MB body limit. |

The v3 MCP server (Phase 10) already enforces Bearer-token + 127.0.0.1
binding + path-traversal blocking. Phase 10 of v5.0 adds the full
OAuth Resource Server pattern + RFC 8707 + the remaining table rows.
