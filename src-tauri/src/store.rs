//! Local cache of the last good reading per provider, plus enough balance
//! history to derive spend for providers that only report a remaining balance.
//!
//! Holding the last good snapshot is what lets the bar keep showing a number
//! when the network drops, flagged `Stale`, instead of flashing to zero.
//!
//! No credential is ever written here.

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::sync::Mutex;

use crate::providers::{ProviderId, Status, UsageSnapshot};

pub struct Store {
    conn: Mutex<Connection>,
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("serialisation error: {0}")]
    Json(#[from] serde_json::Error),
}

impl Store {
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS latest (
                provider   TEXT PRIMARY KEY,
                fetched_at INTEGER NOT NULL,
                payload    TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS balance_history (
                provider      TEXT NOT NULL,
                at            INTEGER NOT NULL,
                balance_cents INTEGER NOT NULL,
                PRIMARY KEY (provider, at)
            );
            CREATE INDEX IF NOT EXISTS balance_history_at
                ON balance_history (provider, at DESC);
            "#,
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn in_memory() -> Result<Self, StoreError> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(
            r#"
            CREATE TABLE latest (
                provider TEXT PRIMARY KEY, fetched_at INTEGER NOT NULL, payload TEXT NOT NULL);
            CREATE TABLE balance_history (
                provider TEXT NOT NULL, at INTEGER NOT NULL, balance_cents INTEGER NOT NULL,
                PRIMARY KEY (provider, at));
            "#,
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn put(&self, snap: &UsageSnapshot) -> Result<(), StoreError> {
        let payload = serde_json::to_string(snap)?;
        let conn = self.conn.lock().expect("store mutex");
        conn.execute(
            "INSERT INTO latest (provider, fetched_at, payload) VALUES (?1, ?2, ?3)
             ON CONFLICT(provider) DO UPDATE SET fetched_at = ?2, payload = ?3",
            params![snap.provider.as_str(), snap.fetched_at.timestamp(), payload],
        )?;

        // Only a live reading from the provider belongs in the history that
        // derived spend is computed from. A `Stale` row is last tick's number
        // repeated, and an `Unsupported` one is a figure the user typed into a
        // budget field — reading a lowered budget as money spent is exactly the
        // kind of confident wrong number this app exists to avoid.
        if let (Some(balance), Status::Ok) = (snap.balance_cents, snap.status) {
            conn.execute(
                "INSERT OR REPLACE INTO balance_history (provider, at, balance_cents)
                 VALUES (?1, ?2, ?3)",
                params![snap.provider.as_str(), snap.fetched_at.timestamp(), balance],
            )?;
            // 90 days is plenty for a 30-day window and keeps the file tiny.
            conn.execute(
                "DELETE FROM balance_history WHERE provider = ?1 AND at < ?2",
                params![
                    snap.provider.as_str(),
                    (Utc::now() - chrono::Duration::days(90)).timestamp()
                ],
            )?;
        }
        Ok(())
    }

    pub fn latest(&self, id: ProviderId) -> Result<Option<UsageSnapshot>, StoreError> {
        let conn = self.conn.lock().expect("store mutex");
        let payload: Option<String> = conn
            .query_row(
                "SELECT payload FROM latest WHERE provider = ?1",
                params![id.as_str()],
                |row| row.get(0),
            )
            .optional()?;
        match payload {
            Some(p) => Ok(Some(serde_json::from_str(&p)?)),
            None => Ok(None),
        }
    }

    pub fn all_latest(&self) -> Result<Vec<UsageSnapshot>, StoreError> {
        let conn = self.conn.lock().expect("store mutex");
        let mut stmt = conn.prepare("SELECT payload FROM latest")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(serde_json::from_str(&row?)?);
        }
        Ok(out)
    }

    /// Return the last good reading marked `Stale`, for use when a fetch fails.
    /// Keeping the number on screen with an honest badge beats blanking it.
    pub fn stale_fallback(
        &self,
        id: ProviderId,
        message: String,
    ) -> Result<UsageSnapshot, StoreError> {
        Ok(match self.latest(id)? {
            Some(mut prev) => {
                prev.status = Status::Stale;
                prev.message = Some(crate::providers::redact(&message));
                prev
            }
            None => UsageSnapshot::empty(id, Status::Error).with_message(message),
        })
    }

    /// Spend derived from a falling prepaid balance, for providers that report
    /// no cost figure at all (DeepSeek).
    ///
    /// Only decreases count. A top-up raises the balance, and treating that as
    /// negative spend would show a credit purchase as money earned.
    pub fn derived_spend_cents(
        &self,
        id: ProviderId,
        since: DateTime<Utc>,
    ) -> Result<Option<i64>, StoreError> {
        let conn = self.conn.lock().expect("store mutex");
        let mut stmt = conn.prepare(
            "SELECT balance_cents FROM balance_history
             WHERE provider = ?1 AND at >= ?2 ORDER BY at ASC",
        )?;
        let readings: Vec<i64> = stmt
            .query_map(params![id.as_str(), since.timestamp()], |row| row.get(0))?
            .collect::<Result<_, _>>()?;

        if readings.len() < 2 {
            return Ok(None);
        }
        let spent = readings
            .windows(2)
            .filter_map(|w| (w[0] > w[1]).then(|| w[0] - w[1]))
            .sum();
        Ok(Some(spent))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::TokenBreakdown;

    fn snap(id: ProviderId, balance: Option<i64>, at: DateTime<Utc>) -> UsageSnapshot {
        let mut s = UsageSnapshot::empty(id, Status::Ok);
        s.balance_cents = balance;
        s.fetched_at = at;
        s.tokens = TokenBreakdown {
            input: 1,
            output: 2,
            ..Default::default()
        };
        s
    }

    #[test]
    fn round_trips_a_snapshot() {
        let store = Store::in_memory().unwrap();
        let s = snap(ProviderId::Anthropic, None, Utc::now());
        store.put(&s).unwrap();
        let back = store.latest(ProviderId::Anthropic).unwrap().unwrap();
        assert_eq!(back.tokens.output, 2);
        assert_eq!(back.status, Status::Ok);
    }

    #[test]
    fn put_is_idempotent_per_provider() {
        let store = Store::in_memory().unwrap();
        store
            .put(&snap(ProviderId::Openai, None, Utc::now()))
            .unwrap();
        store
            .put(&snap(ProviderId::Openai, None, Utc::now()))
            .unwrap();
        assert_eq!(store.all_latest().unwrap().len(), 1);
    }

    #[test]
    fn fallback_marks_the_previous_reading_stale() {
        let store = Store::in_memory().unwrap();
        store
            .put(&snap(ProviderId::Anthropic, None, Utc::now()))
            .unwrap();
        let out = store
            .stale_fallback(ProviderId::Anthropic, "network down".into())
            .unwrap();
        assert_eq!(out.status, Status::Stale);
        assert_eq!(out.tokens.output, 2, "kept the last good numbers");
    }

    #[test]
    fn fallback_without_history_is_an_error_not_a_silent_zero() {
        let store = Store::in_memory().unwrap();
        let out = store
            .stale_fallback(ProviderId::Groq, "boom".into())
            .unwrap();
        assert_eq!(out.status, Status::Error);
    }

    #[test]
    fn derived_spend_ignores_top_ups() {
        let store = Store::in_memory().unwrap();
        let t0 = Utc::now() - chrono::Duration::hours(3);
        // 10.00 -> 9.00 (spent 1.00) -> 20.00 (top-up) -> 19.50 (spent 0.50)
        for (i, bal) in [1000, 900, 2000, 1950].into_iter().enumerate() {
            store
                .put(&snap(
                    ProviderId::Deepseek,
                    Some(bal),
                    t0 + chrono::Duration::minutes(i as i64 * 10),
                ))
                .unwrap();
        }
        let spent = store
            .derived_spend_cents(ProviderId::Deepseek, t0 - chrono::Duration::hours(1))
            .unwrap();
        assert_eq!(spent, Some(150));
    }

    /// Groq and Mistral publish no API at all: their snapshot's "balance" is
    /// `budget_usd` minus what the user typed. Recording that as balance history
    /// made lowering a budget look like money spent.
    #[test]
    fn a_manual_entry_balance_is_not_treated_as_history() {
        let store = Store::in_memory().unwrap();
        let t0 = Utc::now() - chrono::Duration::hours(2);
        for (i, budget) in [5000, 2000].into_iter().enumerate() {
            let mut s = snap(
                ProviderId::Groq,
                Some(budget),
                t0 + chrono::Duration::minutes(i as i64 * 10),
            );
            s.status = Status::Unsupported;
            store.put(&s).unwrap();
        }
        assert_eq!(
            store
                .derived_spend_cents(ProviderId::Groq, t0 - chrono::Duration::hours(1))
                .unwrap(),
            None,
            "editing a budget was read as $30 of spend"
        );
    }

    /// A stale row is the previous reading repeated. Storing it inflates the
    /// history without adding information.
    #[test]
    fn a_stale_reading_is_not_recorded_twice() {
        let store = Store::in_memory().unwrap();
        let t0 = Utc::now() - chrono::Duration::hours(1);
        store.put(&snap(ProviderId::Deepseek, Some(1000), t0)).unwrap();

        let mut repeated = snap(
            ProviderId::Deepseek,
            Some(1000),
            t0 + chrono::Duration::minutes(10),
        );
        repeated.status = Status::Stale;
        store.put(&repeated).unwrap();

        assert_eq!(
            store
                .derived_spend_cents(ProviderId::Deepseek, t0 - chrono::Duration::hours(1))
                .unwrap(),
            None,
            "the stale repeat was stored as a second reading"
        );
    }

    #[test]
    fn derived_spend_needs_two_readings() {
        let store = Store::in_memory().unwrap();
        store
            .put(&snap(ProviderId::Deepseek, Some(500), Utc::now()))
            .unwrap();
        assert_eq!(
            store
                .derived_spend_cents(ProviderId::Deepseek, Utc::now() - chrono::Duration::days(1))
                .unwrap(),
            None
        );
    }
}
