//! The record of every key issued: shown in the History screen, searched,
//! exported to CSV, and used to find customers after a signing-key compromise.

use medflow_license::calendar::{civil_from_days, MS_PER_DAY};
use medflow_license::EXPIRY_WARNING_MS;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssuedKey {
    pub license_id: String,
    /// The full `MFK2.…` key (also the `.mflic` file content).
    pub key: String,
    pub licensee: String,
    pub phone: String,
    pub machine_code: String,
    pub features: Vec<String>,
    pub issued_at: i64,
    pub expires_at: Option<i64>,
    /// `months` / `years` / `days` / `lifetime`, as chosen in the form.
    pub term_kind: String,
    pub term_count: Option<u32>,
    pub kid: String,
    /// `licenseId` of the key this one renews, when issued via "Renew".
    #[serde(default)]
    pub renewal_of: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyState {
    Lifetime,
    Active,
    /// ≤ 14 days left — the same window in which the desktop starts warning.
    Expiring,
    Expired,
}

/// A history row plus its state at "now", as the UI receives it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryItem {
    #[serde(flatten)]
    pub entry: IssuedKey,
    pub state: KeyState,
    /// Whole days left (rounded up); `None` for lifetime.
    pub days_left: Option<i64>,
}

pub fn state_at(expires_at: Option<i64>, now_ms: i64) -> KeyState {
    match expires_at {
        None => KeyState::Lifetime,
        Some(e) if now_ms >= e => KeyState::Expired,
        Some(e) if e - now_ms <= EXPIRY_WARNING_MS => KeyState::Expiring,
        Some(_) => KeyState::Active,
    }
}

pub fn item(entry: &IssuedKey, now_ms: i64) -> HistoryItem {
    HistoryItem {
        entry: entry.clone(),
        state: state_at(entry.expires_at, now_ms),
        days_left: entry
            .expires_at
            .map(|e| ((e - now_ms).max(0) + MS_PER_DAY - 1) / MS_PER_DAY),
    }
}

fn fold(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .collect()
}

/// Case-, space- and dash-insensitive search over name, phone and machine code.
pub fn matches(entry: &IssuedKey, query: &str) -> bool {
    let q = fold(query);
    q.is_empty()
        || fold(&entry.licensee).contains(&q)
        || fold(&entry.phone).contains(&q)
        || fold(&entry.machine_code).contains(&q)
}

/// `YYYY-MM-DD` (UTC).
pub fn iso_date(unix_ms: i64) -> String {
    let (y, m, d) = civil_from_days(unix_ms.div_euclid(MS_PER_DAY));
    format!("{y:04}-{m:02}-{d:02}")
}

fn csv_cell(value: &str) -> String {
    // Spreadsheet formula injection: a cell starting with = + - @ is data, not
    // a formula (a phone like +213… would otherwise be evaluated).
    let guarded = if value.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{value}")
    } else {
        value.to_string()
    };
    format!("\"{}\"", guarded.replace('"', "\"\""))
}

/// UTF-8 with BOM, CRLF, quoted cells — opens cleanly in Excel with accents.
pub fn to_csv(entries: &[IssuedKey], now_ms: i64) -> String {
    let mut out = String::from("\u{feff}");
    let header = [
        "licensee", "phone", "machine_code", "duration", "issued_on", "expires_on", "features",
        "status", "license_id", "kid", "renewal_of", "key",
    ];
    out.push_str(&header.map(csv_cell).join(","));
    out.push_str("\r\n");
    for e in entries {
        let duration = match e.term_count {
            Some(n) => format!("{n} {}", e.term_kind),
            None => e.term_kind.clone(),
        };
        let state = match state_at(e.expires_at, now_ms) {
            KeyState::Lifetime => "lifetime",
            KeyState::Active => "active",
            KeyState::Expiring => "expiring",
            KeyState::Expired => "expired",
        };
        let row = [
            e.licensee.clone(),
            e.phone.clone(),
            e.machine_code.clone(),
            duration,
            iso_date(e.issued_at),
            e.expires_at.map(iso_date).unwrap_or_else(|| "lifetime".into()),
            e.features.join(" "),
            state.to_string(),
            e.license_id.clone(),
            e.kid.clone(),
            e.renewal_of.clone().unwrap_or_default(),
            e.key.clone(),
        ];
        out.push_str(&row.iter().map(|c| csv_cell(c)).collect::<Vec<_>>().join(","));
        out.push_str("\r\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, phone: &str, expires_at: Option<i64>) -> IssuedKey {
        IssuedKey {
            license_id: "id-1".into(),
            key: "MFK2.a.b".into(),
            licensee: name.into(),
            phone: phone.into(),
            machine_code: "MF-MQ6X-JKXA-7TR5-DFNM".into(),
            features: vec!["telegram".into()],
            issued_at: 1_790_000_000_000,
            expires_at,
            term_kind: "months".into(),
            term_count: Some(3),
            kid: "prod-2026-09".into(),
            renewal_of: None,
        }
    }

    #[test]
    fn states() {
        let now = 1_790_000_000_000;
        assert_eq!(state_at(None, now), KeyState::Lifetime);
        assert_eq!(state_at(Some(now), now), KeyState::Expired);
        assert_eq!(state_at(Some(now + 3 * MS_PER_DAY), now), KeyState::Expiring);
        assert_eq!(state_at(Some(now + 40 * MS_PER_DAY), now), KeyState::Active);
        assert_eq!(item(&entry("a", "", Some(now + MS_PER_DAY / 2)), now).days_left, Some(1));
    }

    #[test]
    fn search_ignores_case_spaces_and_dashes() {
        let e = entry("Dr Amina Benali", "0555 12 34 56", None);
        assert!(matches(&e, "amina"));
        assert!(matches(&e, "0555123456"));
        assert!(matches(&e, "mq6xjkxa"));
        assert!(matches(&e, ""));
        assert!(!matches(&e, "karim"));
    }

    #[test]
    fn csv_quotes_and_guards_formulas() {
        let csv = to_csv(&[entry("Dr \"Kim\", Oran", "+213555", None)], 0);
        assert!(csv.starts_with('\u{feff}'));
        assert!(csv.contains("\"Dr \"\"Kim\"\", Oran\""));
        assert!(csv.contains("\"'+213555\""));
        assert!(csv.contains("\"lifetime\""));
        assert_eq!(csv.matches("\r\n").count(), 2);
        assert_eq!(iso_date(1_790_000_000_000), "2026-09-21");
    }
}
