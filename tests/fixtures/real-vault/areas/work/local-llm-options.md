---
title: Local LLM Options (May 2026)
tags: [llm, local, ollama]
---

# Local LLM Options (May 2026)

What's actually runnable locally without a beefy GPU. For Aura's
"private path" + Phase 8 fallback.

| Model | Size | RAM (Q4) | Use case |
|-------|------|----------|----------|
| Phi-3-mini 3.8B | 2.2 GB | 3 GB | General reasoning fallback. Path C in Phase 8. |
| Gemma 3 2B | 1.5 GB | 2 GB | Tiny + fast classifier for query-mode routing. |
| Mistral 7B | 4.5 GB | 6 GB | Heavier local reasoning if user has the RAM. |
| Whisper-tiny | 75 MB | 200 MB | Audio encoder (Phase 9). |
| SigLIP-small | 200 MB | 500 MB | Image encoder (Phase 9). |
| all-MiniLM-L6-v2 | 90 MB | 250 MB | English embeddings (Phase 5). |
| BGE-M3 | 2.3 GB | 3 GB | Multilingual embeddings — Arabic + English. |

## Defaults

- Embedding: BGE-M3 if the user wants Arabic; MiniLM otherwise.
- Audio: Whisper-tiny (good enough for filename-derivable speech).
- Vision: SigLIP-small.
- Reasoning fallback: Phi-3-mini through Ollama if installed.

## Path to enable

User installs Ollama → Aura detects 127.0.0.1:11434 → exposes
"Local LLM ready" badge in the Integrations panel.
