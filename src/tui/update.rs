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
