//! Input rules shared by setup and settings: the vault password and the
//! signing key's name (kid). The webview mirrors the password checklist for
//! live feedback; this module is the authority.

pub const MIN_PASSWORD_CHARS: usize = 10;
pub const MAX_PASSWORD_CHARS: usize = 256;

const COMMON: &[&str] = &[
    "password12", "password123", "motdepasse1", "azerty1234", "azertyuiop1", "qwerty1234",
    "1234567890", "0123456789", "medflow123", "medflow2026", "keymaker123", "algerie2026",
];

/// Every rule the password breaks (empty = accepted).
pub fn password_problems(password: &str) -> Vec<&'static str> {
    let mut problems = Vec::new();
    let chars = password.chars().count();
    if chars < MIN_PASSWORD_CHARS {
        problems.push("too_short");
    }
    if chars > MAX_PASSWORD_CHARS {
        problems.push("too_long");
    }
    if !password.chars().any(char::is_alphabetic) {
        problems.push("needs_letter");
    }
    if !password.chars().any(|c| c.is_ascii_digit()) {
        problems.push("needs_digit");
    }
    let lower = password.to_lowercase();
    let first = lower.chars().next();
    let all_same = first.is_some_and(|f| lower.chars().all(|c| c == f));
    if all_same || COMMON.contains(&lower.as_str()) {
        problems.push("too_common");
    }
    problems
}

/// kid: 3–32 of `a-z 0-9 -`, starting with a letter or digit.
pub fn kid_is_valid(kid: &str) -> bool {
    let len = kid.len();
    (3..=32).contains(&len)
        && kid.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !kid.starts_with('-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_rules() {
        assert!(password_problems("Tlemcen2026!").is_empty());
        assert_eq!(password_problems("short1"), vec!["too_short"]);
        assert_eq!(password_problems("onlyletters"), vec!["needs_digit"]);
        assert_eq!(password_problems("12345678901"), vec!["needs_letter"]);
        assert_eq!(password_problems("Azerty1234"), vec!["too_common"]);
        assert!(password_problems("aaaaaaaaaa").contains(&"too_common"));
        // Accented letters count as letters.
        assert!(password_problems("médecin-2026").is_empty());
    }

    #[test]
    fn kid_rules() {
        assert!(kid_is_valid("prod-2026-09"));
        assert!(!kid_is_valid("Prod-2026"));
        assert!(!kid_is_valid("ab"));
        assert!(!kid_is_valid("-prod"));
        assert!(!kid_is_valid("prod 2026"));
    }
}
