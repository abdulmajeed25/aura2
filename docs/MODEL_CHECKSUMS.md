# Model Checksums

Downloads land under `<vault>/.aura/models/<name>/`. Checksums recorded
2026-05-20 from the autonomous-batch run.

## Verification policy

- **LFS files** (the ONNX weights + the large tokenizer files): HuggingFace
  publishes `lfs.oid` over the tree API. `oid` is the SHA-256 of the LFS
  object, so comparing local `sha256sum` against the API result is an
  end-to-end content check. **All ONNX weights below verified ✅.**
- **Non-LFS files** (small `config.json`, `preprocessor_config.json`):
  HuggingFace exposes the **git blob SHA-1** for these, not SHA-256.
  We record the local SHA-256 as a tripwire so a future re-pull that
  changes the bytes is caught; we do not claim a cross-source verify.

The HF tree API used: `https://huggingface.co/api/models/<repo>/tree/main?recursive=true`.

## `multilingual-e5-small/`

Source: `intfloat/multilingual-e5-small`

| File                     | Bytes        | SHA-256 (local)                                                    | HF LFS oid match |
|--------------------------|-------------:|--------------------------------------------------------------------|:----------------:|
| `model.onnx`             | 470,268,510  | `ca456c06b3a9505ddfd9131408916dd79290368331e7d76bb621f1cba6bc8665` | ✅ `onnx/model.onnx` |
| `tokenizer.json`         |  17,082,730  | `0b44a9d7b51c3c62626640cda0e2c2f70fdacdc25bbbd68038369d14ebdf4c39` | ✅ `onnx/tokenizer.json` |
| `config.json`            |         655  | `69137736cab8b8903a07fe8afaafdda25aac55415a12a55d1bffa9f581abf959` | n/a (non-LFS)   |

## `whisper-tiny/`

Source: `Xenova/whisper-tiny`

| File                       | Bytes       | SHA-256 (local)                                                    | HF LFS oid match |
|----------------------------|------------:|--------------------------------------------------------------------|:----------------:|
| `encoder_model.onnx`       |  32,909,539 | `39e81b6c86a5b2b4beda1bb3145486a769d594801f780a66cad1ae72c7ad2c5e` | ✅ `onnx/encoder_model.onnx` |
| `tokenizer.json`           |   2,480,466 | `27fc476bfe7f17299480be2273fc0608e4d5a99aba2ab5dec5374b4482d1a566` | (not in LFS tree) |
| `config.json`              |       2,248 | `2b2e4e519084e0ea028b19b153f95202735a971870d6844aa26e559edd292e94` | n/a (non-LFS) |
| `preprocessor_config.json` |         339 | `a6a76d28c93edb273669eb9e0b0636a2bddbb1272c3261e47b7ca6dfdbac1b8d` | n/a (non-LFS) |

## `siglip-base/`

Source: `Xenova/siglip-base-patch16-224`

| File                       | Bytes        | SHA-256 (local)                                                    | HF LFS oid match |
|----------------------------|-------------:|--------------------------------------------------------------------|:----------------:|
| `vision_model.onnx`        | 371,819,850  | `f89d41bac7f4d4b87e010a467d93f98689d708916ed22f5a07f96fdfa26f475f` | ✅ `onnx/vision_model.onnx` |
| `tokenizer.json`           |   2,398,744  | `4a17c975210be5ab4c36b47d8dae4eefb866dbfb1e676e394aad85dc30a3ae08` | (not in LFS tree) |
| `preprocessor_config.json` |         368  | `21ee046a8a52a65e5f9c177bf840bfb39ea66c9c54cf2760630efd58e0a3ec80` | n/a (non-LFS) |

## Reproduce

```bash
M=<vault>/.aura/models
find $M -type f \( -name "*.onnx" -o -name "*.json" \) | sort | xargs sha256sum
curl -s "https://huggingface.co/api/models/intfloat/multilingual-e5-small/tree/main?recursive=true" \
  | jq -r '.[] | select(.lfs != null) | "\(.lfs.oid)  \(.path)"'
```
