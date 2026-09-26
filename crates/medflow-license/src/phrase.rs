//! 24-word recovery phrase for the 32-byte Ed25519 signing seed.
//!
//! The seed IS the BIP39 entropy (256 bits + an 8-bit SHA-256 checksum = 24
//! English words). This is NOT the BIP39 "mnemonic → PBKDF2 seed" wallet
//! derivation: decoding the phrase gives back the exact 32 bytes, so restoring
//! on a new phone recreates the identical signing key and the same public key
//! the desktop already trusts.

use bip39::{Language, Mnemonic};
use std::fmt;

pub const PHRASE_WORDS: usize = 24;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PhraseError {
    /// Not 24 words.
    WordCount(usize),
    /// A word that is not on the BIP39 English list. `position` is 1-based.
    UnknownWord { position: usize, word: String },
    /// Every word is valid but the checksum fails — a wrong or swapped word.
    BadChecksum,
}

impl fmt::Display for PhraseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PhraseError::WordCount(n) => write!(f, "expected {PHRASE_WORDS} words, got {n}"),
            PhraseError::UnknownWord { position, word } => {
                write!(f, "word {position} ('{word}') is not a recovery-phrase word")
            }
            PhraseError::BadChecksum => {
                write!(f, "the words are valid but the phrase checksum fails")
            }
        }
    }
}

impl std::error::Error for PhraseError {}

/// Encode a seed as 24 space-separated lowercase English words.
pub fn seed_to_phrase(seed: &[u8; 32]) -> String {
    Mnemonic::from_entropy_in(Language::English, seed)
        .expect("32 bytes is valid BIP39 entropy")
        .to_string()
}

/// Decode a phrase back to the seed. Case and extra whitespace are ignored;
/// the BIP39 checksum is always verified.
pub fn phrase_to_seed(phrase: &str) -> Result<[u8; 32], PhraseError> {
    let words: Vec<String> = phrase
        .split_whitespace()
        .map(|w| w.to_lowercase())
        .collect();
    if words.len() != PHRASE_WORDS {
        return Err(PhraseError::WordCount(words.len()));
    }
    let list = Language::English.word_list();
    for (i, word) in words.iter().enumerate() {
        if !list.contains(&word.as_str()) {
            return Err(PhraseError::UnknownWord {
                position: i + 1,
                word: word.clone(),
            });
        }
    }
    let mnemonic = Mnemonic::parse_in_normalized(Language::English, &words.join(" "))
        .map_err(|_| PhraseError::BadChecksum)?;
    let (entropy, len) = mnemonic.to_entropy_array();
    if len != 32 {
        return Err(PhraseError::WordCount(words.len()));
    }
    let mut seed = [0u8; 32];
    seed.copy_from_slice(&entropy[..32]);
    Ok(seed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_vector_all_zero_entropy() {
        // BIP39 reference vector: 256 zero bits.
        let phrase = seed_to_phrase(&[0u8; 32]);
        assert_eq!(
            phrase,
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon art"
        );
        assert_eq!(phrase_to_seed(&phrase).unwrap(), [0u8; 32]);
    }

    #[test]
    fn round_trips_and_tolerates_case_and_spacing() {
        let seed: [u8; 32] = std::array::from_fn(|i| (i as u8).wrapping_mul(37).wrapping_add(11));
        let phrase = seed_to_phrase(&seed);
        assert_eq!(phrase.split(' ').count(), PHRASE_WORDS);
        let messy = format!("  {}  ", phrase.to_uppercase().replace(' ', " \n "));
        assert_eq!(phrase_to_seed(&messy).unwrap(), seed);
    }

    #[test]
    fn errors_name_the_problem() {
        let phrase = seed_to_phrase(&[9u8; 32]);
        let words: Vec<&str> = phrase.split(' ').collect();

        assert_eq!(
            phrase_to_seed(&words[..23].join(" ")),
            Err(PhraseError::WordCount(23))
        );

        let mut unknown = words.clone();
        unknown[4] = "medflow";
        assert_eq!(
            phrase_to_seed(&unknown.join(" ")),
            Err(PhraseError::UnknownWord {
                position: 5,
                word: "medflow".into()
            })
        );

        // Swap two different words: every word is valid, the checksum is not
        // (checked over several swaps — a single swap can collide 1/256).
        let mut caught = 0;
        for j in 1..words.len() {
            if words[0] == words[j] {
                continue;
            }
            let mut swapped = words.clone();
            swapped.swap(0, j);
            if phrase_to_seed(&swapped.join(" ")) == Err(PhraseError::BadChecksum) {
                caught += 1;
            }
        }
        assert!(caught >= 20, "only {caught} swaps caught");
    }
}
