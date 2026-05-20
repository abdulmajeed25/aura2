"""LLMLingua-2 compression sidecar (Phase batch step 5).

Tiny FastAPI service. Aura's Anthropic provider POSTs a long retrieval
prompt to /compress and gets back a compressed string + the ratio.
Single endpoint so the surface is easy to audit; bind to 127.0.0.1
only — Aura is local-first.

Run:
    uvicorn sidecar:app --host 127.0.0.1 --port 8765
"""

from __future__ import annotations

import os

# Block torch from looking for CUDA before any heavyweight imports.
# Recent transformers calls a CUDA warmup helper from the CPU load
# path; without this guard it crashes on systems with no NVIDIA driver
# (the typical local VPS). The env var is the documented way to force
# CPU-only execution.
if "CUDA_VISIBLE_DEVICES" not in os.environ:
    os.environ["CUDA_VISIBLE_DEVICES"] = ""

import logging
from typing import Optional

from fastapi import FastAPI, HTTPException
from pydantic import BaseModel, Field

logger = logging.getLogger("llmlingua-sidecar")
logger.setLevel(logging.INFO)


class CompressRequest(BaseModel):
    text: str
    target_ratio: float = Field(default=0.5, ge=0.05, le=0.95)
    # Forced retention of any of these phrases verbatim.
    force_tokens: list[str] = Field(default_factory=list)


class CompressResponse(BaseModel):
    compressed: str
    original_chars: int
    compressed_chars: int
    ratio: float


_compressor = None


def _load_compressor():
    """Lazy-load PromptCompressor. First call downloads the model
    (~600 MB for the small variant, ~2 GB for xlm-roberta-large) from
    HuggingFace into ~/.cache/huggingface."""
    global _compressor
    if _compressor is not None:
        return _compressor
    from llmlingua import PromptCompressor
    # Default to the multilingual small variant. The user can override
    # with `LLMLINGUA_MODEL` to switch to the large meetingbank variant.
    model = os.environ.get(
        "LLMLINGUA_MODEL",
        "microsoft/llmlingua-2-xlm-roberta-large-meetingbank",
    )
    # Honour an explicit override; otherwise pick CPU when no CUDA
    # device is visible (common on the VPS / CI runners we ship to).
    device_map = os.environ.get("LLMLINGUA_DEVICE")
    if device_map is None:
        try:
            import torch  # type: ignore
            device_map = "cuda" if torch.cuda.is_available() else "cpu"
        except Exception:
            device_map = "cpu"
    logger.info("loading LLMLingua-2 model %s on %s", model, device_map)
    _compressor = PromptCompressor(
        model_name=model,
        use_llmlingua2=True,
        device_map=device_map,
    )
    return _compressor


app = FastAPI(title="LLMLingua-2 sidecar", version="0.1.0")


@app.get("/healthz")
def healthz():
    return {"status": "ok"}


@app.post("/compress", response_model=CompressResponse)
def compress(req: CompressRequest) -> CompressResponse:
    if not req.text.strip():
        raise HTTPException(status_code=400, detail="empty text")
    try:
        c = _load_compressor()
    except Exception as exc:  # noqa: BLE001 — surface loader errors
        logger.exception("model load failed")
        raise HTTPException(status_code=500, detail=f"loader: {exc}") from exc

    result = c.compress_prompt(
        req.text,
        rate=req.target_ratio,
        force_tokens=req.force_tokens,
    )
    compressed: str = result["compressed_prompt"]
    return CompressResponse(
        compressed=compressed,
        original_chars=len(req.text),
        compressed_chars=len(compressed),
        ratio=len(compressed) / max(1, len(req.text)),
    )
