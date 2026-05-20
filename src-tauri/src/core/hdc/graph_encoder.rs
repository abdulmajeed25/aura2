//! Compose a note's content HV with its graph neighbourhood.
//!
//! The combined HV captures both *content* (text bundle) and *structure*
//! (which other notes it points at and is pointed by). Two notes can be
//! very similar in HV space because they share words, OR because they
//! share neighbours — exactly the property HDC is designed for, and what
//! sets it apart from the Phase 5 hash embedding.

use crate::core::hdc::Hypervector;

/// Identity HV for a note, derived deterministically from its `file_id`.
/// Different files always get near-orthogonal HVs.
pub fn note_identity_hv(file_id: &str) -> Hypervector {
    Hypervector::from_token(&format!("note:{}", file_id))
}

/// Combine a note's text HV with permuted identity HVs for its outgoing
/// and incoming neighbours.
///
/// We use `permute(0)` for outgoing edges (positionally encoded "this note
/// links TO X") and `permute(1)` for incoming edges (rotated → orthogonal,
/// "this note is linked FROM Y"). This means a note with outgoing edge to X
/// and an incoming edge from X gets two *different* contributions, exactly
/// as the spec's role-filler binding prescribes.
pub fn encode_note_combined(
    text_hv: &Hypervector,
    outgoing_neighbour_ids: &[String],
    incoming_neighbour_ids: &[String],
) -> Hypervector {
    let mut parts: Vec<Hypervector> = Vec::with_capacity(
        1 + outgoing_neighbour_ids.len() + incoming_neighbour_ids.len(),
    );
    parts.push(text_hv.clone());
    for id in outgoing_neighbour_ids {
        parts.push(note_identity_hv(id));
    }
    for id in incoming_neighbour_ids {
        parts.push(note_identity_hv(id).permute(1));
    }

    let refs: Vec<&Hypervector> = parts.iter().collect();
    Hypervector::bundle(&refs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::hdc::encoder::encode_text;

    #[test]
    fn shared_neighbours_boost_similarity_even_with_disjoint_text() {
        // Two notes with no vocabulary in common but the same neighbour set
        // should end up more similar than two with no shared neighbours.
        let text_a = encode_text("apple banana cherry");
        let text_b = encode_text("xylophone yodel zigzag");
        let text_c = encode_text("kettle lantern mango");

        // A and B both link to the same neighbours.
        let neighbours = vec!["n1".to_string(), "n2".to_string()];
        let empty: Vec<String> = Vec::new();

        let a = encode_note_combined(&text_a, &neighbours, &empty);
        let b = encode_note_combined(&text_b, &neighbours, &empty);
        let c = encode_note_combined(&text_c, &empty, &empty);

        let sim_shared = a.similarity(&b);
        let sim_isolated = a.similarity(&c);
        assert!(
            sim_shared > sim_isolated + 0.02,
            "shared neighbours should pull HVs together: shared={} isolated={}",
            sim_shared,
            sim_isolated
        );
    }

    #[test]
    fn outgoing_and_incoming_edges_to_same_neighbour_are_decorrelated() {
        let text = encode_text("identical body text for both");
        let out_only = encode_note_combined(&text, &["x".to_string()], &[]);
        let in_only = encode_note_combined(&text, &[], &["x".to_string()]);
        // They share the text component so similarity will be > 0, but the
        // directionality contribution must not collapse them into the same HV.
        assert!(out_only != in_only);
    }
}
