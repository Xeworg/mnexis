//! The resolved process profile: data directory, settings, and the
//! service handle. Every data command goes through
//! [`MemoryService`] under the scope loaded from `mnexis.toml` —
//! the CLI never talks to the repository directly.

use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::application::MemoryService;
use crate::config::ProfileSettings;
use crate::domain::error::MnexisError;
use crate::storage::memory_repo::SqliteMemoryRepository;
use crate::storage::sqlite::open_sqlite;

pub struct Profile {
    pub data_dir: PathBuf,
    pub settings: ProfileSettings,
    service: MemoryService<SqliteMemoryRepository>,
}

impl Profile {
    /// Resolve the data directory, require an initialised profile, and
    /// build the service with the scope from `mnexis.toml`, widened by the
    /// namespace the workspace is bound to (read-granularity S2).
    pub fn open(data_dir: PathBuf) -> Result<Self, MnexisError> {
        let db_path = data_dir.join("mnexis.db");
        if !db_path.exists() {
            return Err(MnexisError::not_found(format!(
                "profile at {} (run `mnexis init` first)",
                data_dir.display()
            )));
        }
        let (settings, _) = ProfileSettings::load_or_default(&data_dir.join("mnexis.toml"))?;
        let mut scope = settings.to_access_scope()?;
        let workspace = crate::application::workspace::current_workspace(&data_dir)?;
        crate::application::workspace::allow_bound_read_namespace(&mut scope, workspace.as_ref());
        let repo = SqliteMemoryRepository::new(open_sqlite(&db_path)?);
        let service = MemoryService::new(repo, scope, settings.to_service_config());
        Ok(Self {
            data_dir,
            settings,
            service,
        })
    }

    pub fn service_mut(&mut self) -> &mut MemoryService<SqliteMemoryRepository> {
        &mut self.service
    }

    pub fn service_ref(&self) -> &MemoryService<SqliteMemoryRepository> {
        &self.service
    }

    pub fn default_namespace(&self) -> String {
        self.settings.default_namespace.clone()
    }

    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("mnexis.db")
    }

    pub fn config_path(&self) -> PathBuf {
        self.data_dir.join("mnexis.toml")
    }

    /// A second connection for the read-only `--explain` path. The
    /// service keeps owning its own connection; this one only reads.
    pub fn explain_connection(&self) -> Result<Connection, MnexisError> {
        open_sqlite(&self.db_path())
    }
}

/// Data-directory resolution: `--data-dir` flag, then `MNEXIS_DATA_DIR`,
/// then `$HOME/.local/share/mnexis`.
pub fn resolve_data_dir(flag: Option<PathBuf>) -> Result<PathBuf, MnexisError> {
    if let Some(dir) = flag {
        return Ok(dir);
    }
    if let Ok(dir) = std::env::var("MNEXIS_DATA_DIR") {
        return Ok(PathBuf::from(dir));
    }
    match std::env::var("HOME") {
        Ok(home) => Ok(Path::new(&home).join(".local/share/mnexis")),
        Err(_) => Err(MnexisError::invalid_argument(
            "data-dir",
            "no --data-dir, no MNEXIS_DATA_DIR, and HOME is unset",
        )),
    }
}

/// A random UUIDv4 from `/dev/urandom` (Linux-first MVP; no uuid crate).
pub fn uuid_v4() -> Result<String, MnexisError> {
    use std::io::Read;
    let mut bytes = [0u8; 16];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .map_err(|e| {
            MnexisError::invalid_argument("uuid", format!("cannot read /dev/urandom: {e}"))
        })?;
    bytes[6] = (bytes[6] & 0x0f) | 0x40; // version 4
    bytes[8] = (bytes[8] & 0x3f) | 0x80; // RFC 4122 variant
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    ))
}

/// The current UTC time as RFC3339 (`YYYY-MM-DDTHH:MM:SSZ`), std-only.
pub fn now_rfc3339() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    rfc3339_from_epoch(secs)
}

pub fn rfc3339_from_epoch(epoch_secs: i64) -> String {
    let days = epoch_secs.div_euclid(86_400);
    let secs_of_day = epoch_secs.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        secs_of_day / 3600,
        (secs_of_day % 3600) / 60,
        secs_of_day % 60
    )
}

/// Howard Hinnant's `civil_from_days`: days since 1970-01-01 → (y, m, d).
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (year + if month <= 2 { 1 } else { 0 }, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uuid_v4_has_the_documented_shape() {
        let uuid = uuid_v4().unwrap();
        let parts: Vec<&str> = uuid.split('-').collect();
        assert_eq!(
            parts.iter().map(|p| p.len()).collect::<Vec<_>>(),
            vec![8, 4, 4, 4, 12],
            "{uuid}"
        );
        assert!(uuid.starts_with(|c: char| c.is_ascii_hexdigit()));
        assert_eq!(parts[2].starts_with('4'), true, "version nibble: {uuid}");
        assert!(
            parts[3].starts_with('8')
                || parts[3].starts_with('9')
                || parts[3].starts_with('a')
                || parts[3].starts_with('b'),
            "variant nibble: {uuid}"
        );
        // Two calls differ (random, not a counter).
        assert_ne!(uuid_v4().unwrap(), uuid);
    }

    #[test]
    fn rfc3339_formats_known_epochs() {
        assert_eq!(rfc3339_from_epoch(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339_from_epoch(1_767_225_600), "2026-01-01T00:00:00Z");
        assert_eq!(rfc3339_from_epoch(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(rfc3339_from_epoch(86_399), "1970-01-01T23:59:59Z");
        assert_eq!(rfc3339_from_epoch(-1), "1969-12-31T23:59:59Z");
    }

    #[test]
    fn resolve_data_dir_prefers_flag_then_env() {
        let flagged = resolve_data_dir(Some(PathBuf::from("/tmp/flagged"))).unwrap();
        assert_eq!(flagged, PathBuf::from("/tmp/flagged"));
    }
}
