# ADR 003: Semantic Intelligence Dual-Build Architecture

## Context
As codebase sizes grow, traditional lexical search (BM25 FTS5) struggles with synonym matching and conceptual queries (e.g., searching for "auth provider" but the code uses "CredentialManager"). We need to introduce Semantic Search using embeddings to improve `fog_lookup` relevance. However, adding ML dependencies usually violates our strict constraints:
- Must remain a zero-dependency, lightweight binary for standard usage (<30MB).
- Must have a sub-5ms cold start to satisfy MCP requirements.
- Must avoid C-FFI linking nightmares on cross-platform targets (Windows, macOS ARM).

## Decision
1. **Dual-Build Architecture:** We implement semantic search behind a Cargo feature flag `#[cfg(feature = "embedding")]`. The default build remains a pure SQLite/FTS5 binary.
2. **Library Choice:** We selected `fastembed-rs` over raw `ort` + `tokenizers`. `fastembed-rs` abstracts the ONNX runtime and tokenization, dramatically simplifying the build process and eliminating manual C-FFI linking risks.
3. **Lazy Initialization:** The ONNX model is loaded lazily via `OnceLock`. It is NOT loaded at server startup, preserving the <5ms cold start guardrail.
4. **Hybrid Re-ranking Pipeline:** Instead of using heavy SQLite vector extensions (`sqlite-vss`), we use BM25 to quickly retrieve the top 100 candidates, fetch their pre-computed embeddings, and then run an in-memory Cosine Similarity re-ranking to yield the final Top 30.
5. **Graceful Fallback:** If the `embedding` feature is enabled but the model file (`all-MiniLM-L6-v2-q8.onnx`) is missing, the system gracefully falls back to the standard BM25 results instead of crashing.

## Status
Accepted - Implemented in v0.8.0

## Consequences
- **Positive:** We achieve high-accuracy semantic search without breaking the core architectural promises of the lightweight tier.
- **Negative:** The `embed` build requires developers to manually download and place the ~23MB quantized ONNX model in `~/.fog/models/` before the feature activates. Schema migration adds a `symbol_embeddings` table to the database.
