//! Machine codes: the short, human-typable identity of one computer.
//!
//! `MF-XXXX-XXXX-XXXX-XXXX` — 16 Crockford base32 symbols after a fixed `MF`
//! prefix. The first 14 symbols (70 bits) are a domain-separated SHA-256 of the
//! computer's machine identity; the last 2 symbols are a checksum.
//!
//! The checksum is a weighted sum mod 1021 (a prime just under 32², so it fits
//! in two symbols). With distinct weights 1..=14 and symbol values below 32,
//! every single-symbol typo and every swap of two data symbols changes the sum
//! by a non-zero amount smaller than the modulus, so both are ALWAYS caught;
//! other random errors slip through about 1 time in 1021.
//!
//! Parsing is forgiving about presentation (case, spaces, dashes, a missing
//! `MF-` prefix, and Crockford's O→0 / I,L→1 aliases) and strict about content.

use sha2::{Digest, Sha256};
use std::fmt;
use std::str::FromStr;

pub const MACHINE_CODE_PREFIX: &str = "MF";
const DATA_SYMBOLS: usize = 14;
const CHECK_SYMBOLS: usize = 2;
const TOTAL_SYMBOLS: usize = DATA_SYMBOLS + CHECK_SYMBOLS;
const CHECK_MODULUS: u32 = 1021;
const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
const DERIVATION_DOMAIN: &[u8] = b"medflow/machine-code/v1\0";

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MachineCode {
    canonical: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MachineCodeError {
    Empty,
    /// Wrong number of symbols (excluding the prefix and separators).
    WrongLength { symbols: usize },
    /// A character that is not a Crockford base32 symbol. `position` is 1-based
    /// among the 16 symbols so a UI can point at it.
    InvalidCharacter { character: char, position: usize },
    /// Well-formed but the checksum does not match — almost always a typo.
    ChecksumMismatch,
}

impl MachineCodeError {
    pub fn code(&self) -> &'static str {
        match self {
            MachineCodeError::Empty => "empty",
            MachineCodeError::WrongLength { .. } => "wrong_length",
            MachineCodeError::InvalidCharacter { .. } => "invalid_character",
            MachineCodeError::ChecksumMismatch => "checksum_mismatch",
        }
    }
}

impl fmt::Display for MachineCodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MachineCodeError::Empty => write!(f, "the machine code is empty"),
            MachineCodeError::WrongLength { symbols } => write!(
                f,
                "a machine code has {TOTAL_SYMBOLS} characters after MF-, this one has {symbols}"
            ),
            MachineCodeError::InvalidCharacter {
                character,
                position,
            } => write!(f, "character {position} ('{character}') is not allowed"),
            MachineCodeError::ChecksumMismatch => {
                write!(f, "the machine code has a typo (checksum does not match)")
            }
        }
    }
}

impl std::error::Error for MachineCodeError {}

fn symbol_value(c: char) -> Option<u8> {
    let c = match c.to_ascii_uppercase() {
        'O' => '0',
        'I' | 'L' => '1',
        other => other,
    };
    ALPHABET.iter().position(|&s| s as char == c).map(|v| v as u8)
}

fn checksum(data: &[u8]) -> u32 {
    data.iter()
        .enumerate()
        .map(|(i, &v)| (i as u32 + 1) * u32::from(v))
        .sum::<u32>()
        % CHECK_MODULUS
}

fn format_symbols(values: &[u8; TOTAL_SYMBOLS]) -> String {
    let mut out = String::with_capacity(MACHINE_CODE_PREFIX.len() + TOTAL_SYMBOLS + 4);
    out.push_str(MACHINE_CODE_PREFIX);
    for (i, &v) in values.iter().enumerate() {
        if i % 4 == 0 {
            out.push('-');
        }
        out.push(ALPHABET[v as usize] as char);
    }
    out
}

fn with_checksum(data: [u8; DATA_SYMBOLS]) -> [u8; TOTAL_SYMBOLS] {
    let sum = checksum(&data);
    let mut values = [0u8; TOTAL_SYMBOLS];
    values[..DATA_SYMBOLS].copy_from_slice(&data);
    values[DATA_SYMBOLS] = (sum / 32) as u8;
    values[DATA_SYMBOLS + 1] = (sum % 32) as u8;
    values
}

fn is_separator(c: char) -> bool {
    c.is_whitespace()
        || matches!(
            c,
            '-' | '_' | '.' | '\u{2010}'..='\u{2015}' | '\u{2212}' | '\u{200B}'..='\u{200D}' | '\u{2060}' | '\u{FEFF}'
        )
}

impl MachineCode {
    /// Derive the code for a machine identity (the desktop passes the OS
    /// machine id). Deterministic: the same identity always yields the same code.
    pub fn derive(machine_identity: &str) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(DERIVATION_DOMAIN);
        hasher.update(machine_identity.as_bytes());
        let digest = hasher.finalize();
        let mut data = [0u8; DATA_SYMBOLS];
        for (i, slot) in data.iter_mut().enumerate() {
            // Big-endian bit stream, 5 bits per symbol.
            let bit = i * 5;
            let word = (u16::from(digest[bit / 8]) << 8) | u16::from(digest[bit / 8 + 1]);
            *slot = ((word >> (11 - bit % 8)) & 0x1F) as u8;
        }
        MachineCode {
            canonical: format_symbols(&with_checksum(data)),
        }
    }

    /// Parse user input, catching typos via the checksum.
    pub fn parse(input: &str) -> Result<Self, MachineCodeError> {
        let compact: Vec<char> = input.chars().filter(|&c| !is_separator(c)).collect();
        if compact.is_empty() {
            return Err(MachineCodeError::Empty);
        }
        let prefix: Vec<char> = MACHINE_CODE_PREFIX.chars().collect();
        // Exactly 16 symbols is a bare code even if its data happens to start
        // with "MF"; any other length starting with MF carries the prefix.
        let has_prefix = compact.len() != TOTAL_SYMBOLS
            && compact.len() >= prefix.len()
            && compact[..prefix.len()]
                .iter()
                .zip(&prefix)
                .all(|(a, b)| a.to_ascii_uppercase() == *b);
        let symbols = if has_prefix {
            &compact[prefix.len()..]
        } else {
            &compact[..]
        };
        if symbols.len() != TOTAL_SYMBOLS {
            return Err(MachineCodeError::WrongLength {
                symbols: symbols.len(),
            });
        }
        let mut values = [0u8; TOTAL_SYMBOLS];
        for (i, &c) in symbols.iter().enumerate() {
            values[i] = symbol_value(c).ok_or(MachineCodeError::InvalidCharacter {
                character: c,
                position: i + 1,
            })?;
        }
        let sum = checksum(&values[..DATA_SYMBOLS]);
        if u32::from(values[DATA_SYMBOLS]) * 32 + u32::from(values[DATA_SYMBOLS + 1]) != sum {
            return Err(MachineCodeError::ChecksumMismatch);
        }
        Ok(MachineCode {
            canonical: format_symbols(&values),
        })
    }

    /// Canonical form: `MF-XXXX-XXXX-XXXX-XXXX`.
    pub fn as_str(&self) -> &str {
        &self.canonical
    }
}

impl fmt::Display for MachineCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.canonical)
    }
}

impl FromStr for MachineCode {
    type Err = MachineCodeError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        MachineCode::parse(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn symbols_of(code: &MachineCode) -> Vec<char> {
        code.as_str()[3..].chars().filter(|&c| c != '-').collect()
    }

    fn rebuild(symbols: &[char]) -> String {
        let text: String = symbols.iter().collect();
        format!(
            "MF-{}-{}-{}-{}",
            &text[0..4],
            &text[4..8],
            &text[8..12],
            &text[12..16]
        )
    }

    #[test]
    fn derivation_is_deterministic_grouped_and_parses_back() {
        let a = MachineCode::derive("4c4c4544-0042-3510-8052-b4c04f4e4d32");
        let b = MachineCode::derive("4c4c4544-0042-3510-8052-b4c04f4e4d32");
        assert_eq!(a, b);
        assert_eq!(a.as_str().len(), "MF-XXXX-XXXX-XXXX-XXXX".len());
        assert!(a.as_str().starts_with("MF-"));
        assert_eq!(MachineCode::parse(a.as_str()).unwrap(), a);
        assert_ne!(a, MachineCode::derive("another-machine"));
    }

    #[test]
    fn parsing_forgives_presentation() {
        let code = MachineCode::derive("presentation");
        let bare: String = symbols_of(&code).into_iter().collect();
        for input in [
            bare.clone(),
            bare.to_lowercase(),
            format!("  mf {}  ", bare),
            code.as_str().replace('-', " "),
            code.as_str().replace('-', "\u{2013}"),
            format!("{}\u{200B}", code.as_str()),
        ] {
            assert_eq!(MachineCode::parse(&input).as_ref(), Ok(&code), "{input:?}");
        }
    }

    #[test]
    fn crockford_aliases_map_to_digits() {
        // Find a code containing a 0 or 1 and type it with the look-alike letter.
        for n in 0..500 {
            let code = MachineCode::derive(&format!("alias-{n}"));
            if let Some(pos) = code.as_str().find('0') {
                let typed = format!("{}O{}", &code.as_str()[..pos], &code.as_str()[pos + 1..]);
                assert_eq!(MachineCode::parse(&typed).unwrap(), code);
                return;
            }
        }
        panic!("no code with a zero in 500 samples");
    }

    #[test]
    fn every_single_substitution_is_detected() {
        let code = MachineCode::derive("substitution");
        let symbols = symbols_of(&code);
        for position in 0..symbols.len() {
            for &replacement in ALPHABET.iter() {
                let replacement = replacement as char;
                if replacement == symbols[position] {
                    continue;
                }
                let mut typo = symbols.clone();
                typo[position] = replacement;
                assert_eq!(
                    MachineCode::parse(&rebuild(&typo)),
                    Err(MachineCodeError::ChecksumMismatch),
                    "position {position} -> {replacement}"
                );
            }
        }
    }

    #[test]
    fn every_transposition_of_data_symbols_is_detected() {
        for n in 0..20 {
            let code = MachineCode::derive(&format!("transpose-{n}"));
            let symbols = symbols_of(&code);
            for i in 0..DATA_SYMBOLS {
                for j in (i + 1)..DATA_SYMBOLS {
                    if symbols[i] == symbols[j] {
                        continue;
                    }
                    let mut swapped = symbols.clone();
                    swapped.swap(i, j);
                    assert_eq!(
                        MachineCode::parse(&rebuild(&swapped)),
                        Err(MachineCodeError::ChecksumMismatch)
                    );
                }
            }
        }
    }

    #[test]
    fn malformed_inputs_report_why() {
        assert_eq!(MachineCode::parse("  - "), Err(MachineCodeError::Empty));
        assert_eq!(
            MachineCode::parse("MF-1234-5678"),
            Err(MachineCodeError::WrongLength { symbols: 8 })
        );
        let code = MachineCode::derive("bad-char");
        let mut symbols = symbols_of(&code);
        symbols[5] = 'U';
        assert_eq!(
            MachineCode::parse(&rebuild(&symbols)),
            Err(MachineCodeError::InvalidCharacter {
                character: 'U',
                position: 6
            })
        );
    }
}
