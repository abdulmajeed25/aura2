//! Wrapper around the HuggingFace `tokenizers` crate for the all-MiniLM-L6-v2
//! tokenizer.json.

use std::path::Path;

use thiserror::Error;
use tokenizers::Tokenizer;

#[derive(Debug, Error)]
pub enum TokenizerError {
    #[error("failed to load tokenizer.json: {0}")]
    Load(String),
    #[error("failed to encode text: {0}")]
    Encode(String),
}

/// `(input_ids, attention_mask, token_type_ids)`.
pub type EncodedTriple = (Vec<i64>, Vec<i64>, Vec<i64>);

pub struct MiniLmTokenizer {
    inner: Tokenizer,
    /// MiniLM was trained with a sequence length of 256 tokens (after BERT
    /// truncation). Longer inputs are truncated; shorter ones padded.
    pub max_length: usize,
}

impl MiniLmTokenizer {
    pub fn from_file(tokenizer_json: &Path) -> Result<Self, TokenizerError> {
        let inner =
            Tokenizer::from_file(tokenizer_json).map_err(|e| TokenizerError::Load(e.to_string()))?;
        Ok(Self {
            inner,
            max_length: 256,
        })
    }

    /// Tokenize one text. Returns `(input_ids, attention_mask, token_type_ids)`
    /// — the three tensors the all-MiniLM-L6-v2 ONNX graph expects.
    pub fn encode(&self, text: &str) -> Result<EncodedTriple, TokenizerError> {
        let mut encoding = self
            .inner
            .encode(text, true)
            .map_err(|e| TokenizerError::Encode(e.to_string()))?;

        // Truncate to max_length. The tokenizer's own truncation config is set
        // up in tokenizer.json, but we belt-and-brace here in case the json is
        // shipped without it.
        if encoding.get_ids().len() > self.max_length {
            encoding.truncate(self.max_length, 0, tokenizers::TruncationDirection::Right);
        }
        // Pad to max_length so the ONNX graph gets a fixed shape.
        encoding.pad(
            self.max_length,
            0,           // PAD token id (BERT)
            0,           // PAD type id
            "[PAD]",
            tokenizers::PaddingDirection::Right,
        );

        let ids: Vec<i64> = encoding.get_ids().iter().map(|&v| v as i64).collect();
        let mask: Vec<i64> = encoding
            .get_attention_mask()
            .iter()
            .map(|&v| v as i64)
            .collect();
        let type_ids: Vec<i64> = encoding
            .get_type_ids()
            .iter()
            .map(|&v| v as i64)
            .collect();
        Ok((ids, mask, type_ids))
    }
}
