//! Small integration-spike primitives. This is not the production application service.

use rusqlite::{Connection, OpenFlags};
use std::path::Path;
use thiserror::Error;

pub mod mcp;

#[derive(Debug, Error)]
pub enum SpikeError {
    #[error("SQLite operation failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("SQLite FTS5 is unavailable")]
    Fts5Unavailable,
}

/// Opens the canonical local database and verifies FTS5 by creating a temporary virtual table.
pub fn open_sqlite(path: &Path) -> Result<Connection, SpikeError> {
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    connection.pragma_update(None, "journal_mode", "WAL")?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    connection.busy_timeout(std::time::Duration::from_secs(2))?;
    connection
        .execute_batch("CREATE VIRTUAL TABLE IF NOT EXISTS mnexis_fts_probe USING fts5(content);")
        .map_err(|error| match error {
            rusqlite::Error::SqliteFailure(ref code, _)
                if code.code == rusqlite::ErrorCode::Unknown =>
            {
                SpikeError::Fts5Unavailable
            }
            other => SpikeError::Sqlite(other),
        })?;
    Ok(connection)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_sqlite_supports_fts5_and_reopens_profile() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("mnexis.db");
        {
            let connection = open_sqlite(&path).unwrap();
            connection
                .execute(
                    "INSERT INTO mnexis_fts_probe(content) VALUES (?1)",
                    ["petgraph replaced by custom engine"],
                )
                .unwrap();
            let matches: i64 = connection
                .query_row(
                    "SELECT count(*) FROM mnexis_fts_probe WHERE mnexis_fts_probe MATCH 'petgraph'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(matches, 1);
        }
        let connection = open_sqlite(&path).unwrap();
        let count: i64 = connection
            .query_row("SELECT count(*) FROM mnexis_fts_probe", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn independent_connections_serialize_writes_without_losing_committed_data() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("shared.db");
        let first = open_sqlite(&path).unwrap();
        let second = open_sqlite(&path).unwrap();
        first
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS writes (id INTEGER PRIMARY KEY, body TEXT NOT NULL);",
            )
            .unwrap();
        first
            .execute("INSERT INTO writes(body) VALUES ('from first')", [])
            .unwrap();
        second
            .execute("INSERT INTO writes(body) VALUES ('from second')", [])
            .unwrap();
        let count: i64 = first
            .query_row("SELECT count(*) FROM writes", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 2);
    }
}
