# LLMLingua-2 sidecar

Tiny FastAPI service that closes stand-in #18 (retrieval-prompt
compression). Aura's Anthropic provider POSTs the assembled retrieval
prompt to `/compress`, gets a shorter version back, and forwards that
to Claude.

## Install

```bash
python3 -m venv .venv
.venv/bin/pip install -r requirements.txt
```

PEP 668 on recent Debian/Ubuntu blocks system-wide pip installs; the
venv keeps the sidecar self-contained.

## Run

```bash
.venv/bin/uvicorn sidecar:app --host 127.0.0.1 --port 8765
```

First `/compress` call downloads the model (default
`microsoft/llmlingua-2-xlm-roberta-large-meetingbank`, ~2 GB) into
`~/.cache/huggingface`. Override the model with `LLMLINGUA_MODEL`.

## API

`POST /compress`
```json
{ "text": "<long prompt>", "target_ratio": 0.5, "force_tokens": [] }
```
Response:
```json
{ "compressed": "...", "original_chars": 1234, "compressed_chars": 600, "ratio": 0.486 }
```

`GET /healthz` returns `{ "status": "ok" }`.

## Why a sidecar

LLMLingua-2 ships as a PyTorch model and the Rust-side options for
serving XLM-RoBERTa inference under load (ort, candle, tract) all
require either a license-encumbered runtime download or trade
accuracy. A 30-line Python service is the lowest-risk seam until the
Rust ecosystem catches up.

The Rust side (`crate::ai::retrieval::llmlingua::SidecarCompressor`)
treats the sidecar as optional: if it's not reachable the prompt
ships uncompressed.
