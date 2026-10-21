//! The update notice of the TUI.
//!
//! The version check is a network call, so it never runs in the event
//! loop: it runs in a thread when the TUI opens, and the menu shows what
//! came back. The installation the user asks for runs the same way, so
//! the interface never freezes while a binary is downloaded.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::application::update::UpdateService;

/// What the TUI knows about a newer release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateNotice {
    /// The check has not answered yet.
    Checking,
    /// A newer release is published.
    Available { version: String },
    /// The installation is running.
    Installing,
    /// The installation finished; the new binary waits for a restart.
    Installed { version: String },
    /// The check or the installation failed.
    Failed { reason: String },
    /// There is no newer release, or none could be resolved.
    None,
}

/// The background work of the update.
pub struct UpdateController {
    data_dir: PathBuf,
    notice: Arc<Mutex<UpdateNotice>>,
}

impl UpdateController {
    /// A controller for the profile in `data_dir`.
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            data_dir,
            notice: Arc::new(Mutex::new(UpdateNotice::Checking)),
        }
    }

    /// What the menu shows.
    pub fn notice(&self) -> UpdateNotice {
        self.notice
            .lock()
            .map(|notice| notice.clone())
            .unwrap_or(UpdateNotice::None)
    }

    fn set(&self, notice: UpdateNotice) {
        if let Ok(mut slot) = self.notice.lock() {
            *slot = notice;
        }
    }

    /// Ask whether a newer release exists, in the background.
    pub fn start_check(&self) {
        let data_dir = self.data_dir.clone();
        let notice = Arc::clone(&self.notice);
        std::thread::spawn(move || {
            let service = UpdateService::new(&data_dir);
            let answer = match service.check(false) {
                Ok(status) => match (status.update_available, status.latest_version) {
                    (true, Some(version)) => UpdateNotice::Available { version },
                    _ => UpdateNotice::None,
                },
                Err(err) => UpdateNotice::Failed {
                    reason: err.to_string(),
                },
            };
            if let Ok(mut slot) = notice.lock() {
                *slot = answer;
            }
        });
    }

    /// Install the newer release, in the background.
    pub fn start_install(&self) {
        let data_dir = self.data_dir.clone();
        let notice = Arc::clone(&self.notice);
        self.set(UpdateNotice::Installing);
        std::thread::spawn(move || {
            let service = UpdateService::new(&data_dir);
            let answer = match service.install(None) {
                Ok(outcome) => UpdateNotice::Installed {
                    version: outcome.to_version,
                },
                Err(err) => UpdateNotice::Failed {
                    reason: err.to_string(),
                },
            };
            if let Ok(mut slot) = notice.lock() {
                *slot = answer;
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_controller_is_checking() {
        let controller = UpdateController::new(PathBuf::from("/tmp"));
        assert_eq!(controller.notice(), UpdateNotice::Checking);
    }

    #[test]
    fn the_menu_does_not_install_when_no_release_was_announced() {
        // The key only acts when the menu announced a release: a
        // regression that drops the gate would install on every `u`.
        let dir = tempfile::tempdir_in("target").expect("tempdir");
        let controller = std::sync::Arc::new(UpdateController::new(dir.path().to_path_buf()));
        let mut app = crate::tui::app::App::new_at_menu(&test_service());
        app.update = Some(std::sync::Arc::clone(&controller));

        // Nothing announced: the notice is still the initial one.
        let mut service = test_service();
        app.apply(crate::tui::app::Action::Update, &mut service);
        assert_eq!(
            controller.notice(),
            UpdateNotice::Checking,
            "the install must not start without an announced release"
        );
    }

    #[test]
    fn the_menu_installs_when_a_release_was_announced() {
        let dir = tempfile::tempdir_in("target").expect("tempdir");
        let controller = std::sync::Arc::new(UpdateController::new(dir.path().to_path_buf()));
        controller.set(UpdateNotice::Available {
            version: "0.2.0".to_string(),
        });
        let mut app = crate::tui::app::App::new_at_menu(&test_service());
        app.update = Some(std::sync::Arc::clone(&controller));

        let mut service = test_service();
        app.apply(crate::tui::app::Action::Update, &mut service);
        assert_ne!(
            controller.notice(),
            UpdateNotice::Available {
                version: "0.2.0".to_string()
            },
            "the install must start when a release was announced"
        );
    }

    fn test_service() -> crate::tui::app::AppService {
        let conn = rusqlite::Connection::open_in_memory().expect("in-memory database");
        crate::storage::sqlite::run_migrations(&conn).expect("migrations");
        let repo = crate::storage::memory_repo::SqliteMemoryRepository::new(conn);
        crate::application::MemoryService::new(
            repo,
            crate::domain::scope::AccessScope::new(
                vec!["ns-a".to_string()],
                vec!["ns-a".to_string()],
                "ns-a".to_string(),
                false,
                vec![crate::domain::scope::Operation::List],
            ),
            crate::application::ServiceConfig::default(),
        )
    }

    #[test]
    fn the_notice_can_be_set() {
        let controller = UpdateController::new(PathBuf::from("/tmp"));
        controller.set(UpdateNotice::Available {
            version: "0.2.0".to_string(),
        });
        assert_eq!(
            controller.notice(),
            UpdateNotice::Available {
                version: "0.2.0".to_string()
            }
        );
    }
}
