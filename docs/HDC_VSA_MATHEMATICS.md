# HDC / VSA Mathematics

Reference for the math used by `src-tauri/src/core/hdc/`.

## Notation

- `D` = hypervector dimension. Current `HV_DIM = 10_000`.
- Bipolar hypervector: `H ∈ {-1, +1}^D`.
- FHRR alternative (Phase 6 research branch): `H ∈ ℂ^D` with `|H_i| = 1`.

## Operations (bipolar)

| Op | Formula | Properties |
|----|---------|-----------|
| `bundle(A, B)` | `sign(A + B)` (with majority threshold for ties) | Commutative, associative, idempotent for identical inputs. Preserves similarity to components above chance. |
| `bind(A, B)` | `A ⊙ B` (Hadamard / element-wise product) | Self-inverse: `bind(A, A) = +1`. Commutative. Distributes over bundling. |
| `permute(A, k)` | cyclic right rotation by `k` positions | `permute(A, 1)` is ~orthogonal to `A`. Used to encode order / role. |
| `similarity(A, B)` | `(A · B) / D` | In `[-1, +1]`. For bipolar HVs, `|A| = |B| = √D` is constant so cosine = dot/D. |
| `unbind(C, A)` | `bind(C, A)` (since bind is self-inverse) | Recovers `B` from `bind(A, B)` up to noise. |

## Graph encoding (role-filler)

```
note_hv  = bundle(token_hv_1, token_hv_2, ...)
edge_hv  = source_hv ⊙ EDGE_TYPE_hv ⊙ permute(target_hv, 1)
note_with_neighborhood_hv = bundle(
    note_hv,
    edge_hv_out_1, edge_hv_out_2, ...,
    permute(edge_hv_in_1, 2), ...
)
```

The asymmetry between outgoing (permute 1) and incoming (permute 2)
edges prevents `A → B` and `B → A` from collapsing into the same
neighbourhood signature.

## Property proven by the v3 codebase

`shared_neighbours_pull_combined_hvs_together` (passes today):
two notes with **disjoint vocabularies** but **shared neighbours**
land at cosine 0.6340 in the combined-HV space, versus -0.0062 for
an orphan note. This is the property that distinguishes HDC from
the Phase 5 hash embedder.

## FHRR (Phase 6 research branch — not yet built)

| Op | Formula |
|----|---------|
| `bind(A, B)` | element-wise complex multiplication |
| `bundle({A_i})` | mean + renormalisation to unit modulus per coordinate |
| `unbind(C, A)` | `bind(C, conj(A))` |
| `similarity(A, B)` | `Re(<A, B>) / D` |

Implementation will use `rustfft` for the convolution variant.
Optional columns `fhrr_real` and `fhrr_imag` are already in the
LanceDB schema design for `block_hypervectors`.

## Test discipline (Math-Driven TDD)

Every numerical kernel ships with a unit test whose expected output
was hand-computed before the implementation, e.g.:

```rust
#[test]
fn bind_inverts_with_itself() {
    let a = Hypervector::from_token("alpha");
    let bb = a.bind(&a);
    assert!(bb.0.iter().all(|&v| v == 1));   // bipolar self-inverse
}
```

These tests are the contract a real ONNX / model swap must preserve.
