//! Terminal-free write forms for `mnexis tui` (S2 save, update, supersede).
//!
//! A form is a list of labelled text fields plus a submit action. It never
//! touches SQL or the terminal: submission builds the request with the same
//! builder the CLI uses (`cli::commands::{save_request, update_request,
//! supersede_request}`) and calls the same [`MemoryService`] method, so the
//! TUI cannot drift from `mnexis save` / `mnexis update` / `mnexis supersede`.
//!
//! The client-side checks only mirror the service's required-field and size
//! limits for fast feedback (limits come from [`ServiceConfig`], never from
//! copied constants). Everything else — topic-key charset, state transition
//! legality, topic uniqueness, scope — stays in the service and is surfaced
//! verbatim as a form error.

use crate::application::{SaveRequest, UpdateRequest};
use crate::cli::commands;
use crate::domain::error::MnexisError;
use crate::domain::memory::Memory;
use crate::tui::app::AppService;

/// States the save form accepts (CLI `--state` surface, S2).
const SAVE_STATES: &[&str] = &["active", "proposed"];

/// What the app does after a successful write: refresh the list, then
/// focus the namespace and the row that changed.
pub struct WriteOutcome {
    pub message: String,
    pub uuid: String,
    pub namespace: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormKind {
    Save,
    Update,
    Supersede,
}

/// One editable row. `multiline` fields take Enter as a newline; every
/// other field treats Enter as "go to the next field".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub label: &'static str,
    pub hint: &'static str,
    pub multiline: bool,
    pub value: String,
}

impl Field {
    fn single(label: &'static str, value: impl Into<String>) -> Self {
        Self {
            label,
            hint: "",
            multiline: false,
            value: value.into(),
        }
    }

    fn multi(label: &'static str, value: impl Into<String>) -> Self {
        Self {
            label,
            hint: "",
            multiline: true,
            value: value.into(),
        }
    }

    fn hint(mut self, hint: &'static str) -> Self {
        self.hint = hint;
        self
    }

    /// Request value: single-line fields are trimmed, multi-line content is
    /// kept verbatim.
    fn text(&self) -> &str {
        if self.multiline {
            &self.value
        } else {
            self.value.trim()
        }
    }
}

/// A save or update form. All fields are public so tests drive it headlessly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Form {
    pub kind: FormKind,
    pub fields: Vec<Field>,
    pub index: usize,
    pub error: Option<String>,
    /// Set after a revision conflict: Ctrl-R reloads the target memory.
    pub reload_offered: bool,
    /// Update target (uuid); `None` for the save form.
    pub target: Option<String>,
    /// Namespace of the write target when the service result cannot supply
    /// it: `supersede` returns `()`, so the form carries the older row's
    /// namespace for the list refresh after the write.
    pub namespace: Option<String>,
}

/// A submission failure. Client-side rejections carry a form message;
/// service rejections keep the typed error so the revision conflict can be
/// rendered with the revision involved.
#[derive(Debug)]
enum SubmitFailure {
    Invalid(String),
    Service(MnexisError),
    /// An optimistic precondition the form mirrors itself because the
    /// service does not check that side yet (supersede's newer revision).
    Conflict {
        current: i64,
        expected: String,
    },
}

impl Form {
    /// The save form, prefilled with the CLI defaults and the currently
    /// selected namespace.
    pub fn save(namespace: &str) -> Self {
        Self {
            kind: FormKind::Save,
            fields: vec![
                Field::multi("content", ""),
                Field::single("namespace", namespace).hint("required"),
                Field::single("type", "fact"),
                Field::single("basis", "user_stated"),
                Field::single("topic_key", "").hint("a-z0-9.- only"),
                Field::single("idempotency_key", ""),
                Field::single("tags", "").hint("comma separated"),
                Field::single("state", "active").hint("active|proposed"),
            ],
            index: 0,
            error: None,
            reload_offered: false,
            target: None,
            namespace: None,
        }
    }

    /// The update form for a loaded memory: content, state, and basis are
    /// prefilled, and `expected_revision` comes from the revision we read.
    /// Tags are read through the service by the caller and prefilled here;
    /// blank still means "keep the current tags" (the service drops an empty
    /// tag list, exactly like `mnexis update` without `--tags`).
    pub fn update(memory: &Memory, tags: &[String]) -> Self {
        Self {
            kind: FormKind::Update,
            fields: vec![
                Field::multi("content", memory.content.clone()),
                Field::single("state", memory.state.as_str()).hint("blank keeps current"),
                Field::single("tags", tags.join(", ")).hint("blank keeps current"),
                Field::single("basis", memory.assertion_basis.clone()),
                Field::single("reason", "").hint("required"),
                Field::single("expected_revision", memory.revision.to_string()),
            ],
            index: 0,
            error: None,
            reload_offered: false,
            target: Some(memory.uuid.clone()),
            namespace: Some(memory.namespace.clone()),
        }
    }

    /// The supersede form, opened after the newer memory has been picked.
    /// Both revisions are prefilled from the rows loaded at pick time and
    /// the uuid pair is editable, so the request is always explicit.
    pub fn supersede(older: &Memory, newer: &Memory) -> Self {
        Self {
            kind: FormKind::Supersede,
            fields: vec![
                Field::single("older_uuid", older.uuid.clone()).hint("required"),
                Field::single("newer_uuid", newer.uuid.clone()).hint("required"),
                Field::single("reason", "").hint("required"),
                Field::single("expected_old_revision", older.revision.to_string()),
                Field::single("expected_new_revision", newer.revision.to_string()),
            ],
            index: 0,
            error: None,
            reload_offered: false,
            target: Some(older.uuid.clone()),
            namespace: Some(older.namespace.clone()),
        }
    }

    pub fn title(&self) -> String {
        match (self.kind, self.target.as_deref()) {
            (FormKind::Save, _) => "Save memory (n)".to_string(),
            (FormKind::Update, Some(uuid)) => {
                format!("Update memory {} (e)", short_uuid(uuid))
            }
            (FormKind::Update, None) => "Update memory (e)".to_string(),
            (FormKind::Supersede, Some(uuid)) => {
                format!("Supersede {} (s)", short_uuid(uuid))
            }
            (FormKind::Supersede, None) => "Supersede memory (s)".to_string(),
        }
    }

    pub fn focused(&self) -> Option<&Field> {
        self.fields.get(self.index)
    }

    /// Trimmed value of `label`, or `""` when the form has no such field.
    pub fn value(&self, label: &str) -> &str {
        self.fields
            .iter()
            .find(|f| f.label == label)
            .map(Field::text)
            .unwrap_or("")
    }

    pub fn is_last(&self) -> bool {
        self.index + 1 >= self.fields.len()
    }

    /// Enter edits instead of navigating when the focused field is
    /// multi-line (the content field).
    pub fn enter_edits(&self) -> bool {
        self.focused().map(|f| f.multiline).unwrap_or(false)
    }

    pub fn next(&mut self) {
        if self.index + 1 < self.fields.len() {
            self.index += 1;
        }
    }

    pub fn prev(&mut self) {
        self.index = self.index.saturating_sub(1);
    }

    pub fn insert(&mut self, c: char) {
        if let Some(field) = self.fields.get_mut(self.index) {
            field.value.push(c);
        }
    }

    pub fn newline(&mut self) {
        if let Some(field) = self.fields.get_mut(self.index) {
            if field.multiline {
                field.value.push('\n');
            }
        }
    }

    pub fn backspace(&mut self) {
        if let Some(field) = self.fields.get_mut(self.index) {
            field.value.pop();
        }
    }

    fn set_value(&mut self, label: &str, value: String) {
        if let Some(field) = self.fields.iter_mut().find(|f| f.label == label) {
            field.value = value;
        }
    }

    /// Re-read the update target (or both supersede sides) and refresh the
    /// revisions this form will submit against, keeping every edit the user
    /// already typed.
    pub fn reload(&mut self, service: &AppService) -> Result<Memory, String> {
        let Some(uuid) = self.target.clone() else {
            return Err("no memory to reload".to_string());
        };
        match self.kind {
            FormKind::Supersede => {
                let older = self.read_for_revision(service, &uuid)?;
                let newer_uuid = self.value("newer_uuid").to_string();
                let newer = self.read_for_revision(service, &newer_uuid)?;
                self.set_value("expected_old_revision", older.revision.to_string());
                self.set_value("expected_new_revision", newer.revision.to_string());
                self.error = None;
                self.reload_offered = false;
                Ok(older)
            }
            _ => match service.get(&uuid) {
                Ok(Some(memory)) => {
                    self.set_value("expected_revision", memory.revision.to_string());
                    self.error = None;
                    self.reload_offered = false;
                    Ok(memory)
                }
                Ok(None) => Err(format!("memory {uuid} not found")),
                Err(e) => Err(format!("reload failed: {e}")),
            },
        }
    }

    fn read_for_revision(&self, service: &AppService, uuid: &str) -> Result<Memory, String> {
        match service.get(uuid) {
            Ok(Some(memory)) => Ok(memory),
            Ok(None) => Err(format!("memory {uuid} not found")),
            Err(e) => Err(format!("reload failed: {e}")),
        }
    }

    /// Validate and write. On failure the form stays open, `error` explains
    /// why, and `reload_offered` marks the revision-conflict recovery path.
    pub fn submit(&mut self, service: &mut AppService) -> Result<WriteOutcome, ()> {
        self.error = None;
        self.reload_offered = false;
        let result = match self.kind {
            FormKind::Save => self.submit_save(service),
            FormKind::Update => self.submit_update(service),
            FormKind::Supersede => self.submit_supersede(service),
        };
        match result {
            Ok(outcome) => Ok(outcome),
            Err(failure) => {
                match &failure {
                    SubmitFailure::Invalid(message) => self.error = Some(message.clone()),
                    SubmitFailure::Conflict { current, expected } => {
                        self.reload_offered = true;
                        self.error = Some(format!(
                            "revision conflict: current is {current} — {expected}, \
                             Ctrl-R reloads the memory"
                        ));
                    }
                    SubmitFailure::Service(err @ MnexisError::RevisionConflict { .. }) => {
                        self.reload_offered = true;
                        // The service error is rendered verbatim, plus the
                        // revision the form submitted with and the revision
                        // currently on disk, so the conflict is actionable.
                        self.error = Some(self.conflict_message(err, service));
                    }
                    SubmitFailure::Service(e) => self.error = Some(e.to_string()),
                }
                Err(())
            }
        }
    }

    /// A conflict is only actionable when it names what the form expected
    /// and what is on disk. The update form owns one revision, the supersede
    /// form two; both get the same message shape.
    fn conflict_message(&self, err: &MnexisError, service: &AppService) -> String {
        let expected = match self.kind {
            FormKind::Supersede => format!(
                "expected older revision {}",
                self.value("expected_old_revision")
            ),
            _ => format!("expected revision {}", self.value("expected_revision")),
        };
        match self.loaded_revision(service) {
            Some(rev) => {
                format!("{err} — {expected}, loaded revision is {rev}; Ctrl-R reloads the memory")
            }
            None => format!("{err} — {expected}; Ctrl-R reloads the memory"),
        }
    }

    /// The target memory's revision on disk, read through the service. Used
    /// only to make a revision conflict message actionable.
    fn loaded_revision(&self, service: &AppService) -> Option<i32> {
        let uuid = self.target.as_deref()?;
        service
            .get(uuid)
            .ok()
            .flatten()
            .map(|memory| memory.revision)
    }

    fn submit_save(&mut self, service: &mut AppService) -> Result<WriteOutcome, SubmitFailure> {
        let content = self.required("content")?;
        check_bytes("content", &content, service.config().max_content_bytes)?;
        let namespace = self.required("namespace")?;
        let memory_type = self.required("type")?;
        let basis = self.required("basis")?;
        let topic_key = self.value("topic_key").to_string();
        let idempotency_key = blank_to_none(self.value("idempotency_key"));
        let tags = self.value("tags").to_string();
        check_tags(&tags, service.config().max_tags)?;
        let state = validate_save_state(self.value("state"))?;
        let req: SaveRequest = commands::save_request(
            "tui",
            namespace.clone(),
            Some(memory_type),
            Some(basis),
            blank_to_none(&topic_key),
            idempotency_key,
            None,
            None,
            blank_to_none(&tags),
            Some(state),
            content,
            None,
        )
        .map_err(SubmitFailure::Service)?;
        let memory = service.save(req).map_err(SubmitFailure::Service)?;
        Ok(WriteOutcome {
            message: format!("saved {} (row {})", memory.uuid, memory.id.unwrap_or(0)),
            uuid: memory.uuid,
            namespace,
        })
    }

    fn submit_update(&mut self, service: &mut AppService) -> Result<WriteOutcome, SubmitFailure> {
        let Some(uuid) = self.target.clone() else {
            return Err(SubmitFailure::Invalid("no memory selected".to_string()));
        };
        let content = self.required("content")?;
        check_bytes("content", &content, service.config().max_content_bytes)?;
        let reason = self.required("reason")?;
        let expected_revision = self.revision("expected_revision")?;
        let tags = self.value("tags").to_string();
        check_tags(&tags, service.config().max_tags)?;
        // State transitions are domain rules: only the parse is mirrored
        // here, the service owns legality (`can_transition_to`).
        let state = match self.value("state") {
            "" => None,
            raw => Some(commands::parse_state(raw).map_err(SubmitFailure::Service)?),
        };
        let basis = blank_to_none(self.value("basis"));
        let req: UpdateRequest = commands::update_request(
            uuid,
            Some(content),
            state.map(|s| s.as_str().to_string()),
            blank_to_none(&tags),
            basis,
            expected_revision,
            reason,
        )
        .map_err(SubmitFailure::Service)?;
        let memory = service.update(req).map_err(SubmitFailure::Service)?;
        Ok(WriteOutcome {
            message: format!(
                "updated {} to revision {} ({})",
                memory.uuid,
                memory.revision,
                memory.state.as_str()
            ),
            uuid: memory.uuid,
            namespace: memory.namespace,
        })
    }

    fn submit_supersede(
        &mut self,
        service: &mut AppService,
    ) -> Result<WriteOutcome, SubmitFailure> {
        let older_uuid = self.required("older_uuid")?;
        let newer_uuid = self.required("newer_uuid")?;
        if older_uuid == newer_uuid {
            // A self-link is the smallest cycle; the service owns the
            // general cycle rule, but this one is cheap to name here.
            return Err(SubmitFailure::Invalid(
                "older_uuid and newer_uuid must differ".to_string(),
            ));
        }
        let reason = self.required("reason")?;
        let expected_old_revision = self.revision("expected_old_revision")?;
        let expected_new_revision = self.revision("expected_new_revision")?;
        self.check_supersede_revisions(service)?;
        let req = commands::supersede_request(
            older_uuid.clone(),
            newer_uuid.clone(),
            expected_old_revision,
            expected_new_revision,
            reason,
        );
        service.supersede(req).map_err(SubmitFailure::Service)?;
        Ok(WriteOutcome {
            message: format!("superseded {older_uuid} with {newer_uuid}"),
            uuid: older_uuid,
            namespace: self.namespace.clone().unwrap_or_default(),
        })
    }

    /// Re-read both sides of the supersession and fail when either drifted
    /// from the revision the form carries. `MemoryService::supersede`
    /// enforces the older side itself; the newer side is mirrored here
    /// until the service checks it too (`docs/contracts/v1/README.md`: the
    /// supersede is "revision-checked on both records"), so a stale picker
    /// can never link silently.
    fn check_supersede_revisions(&self, service: &AppService) -> Result<(), SubmitFailure> {
        for (uuid_label, revision_label, side) in [
            ("older_uuid", "expected_old_revision", "older"),
            ("newer_uuid", "expected_new_revision", "newer"),
        ] {
            let uuid = self.value(uuid_label).to_string();
            let expected = self.revision(revision_label)?;
            match service.get(&uuid) {
                Ok(Some(memory)) if memory.revision != expected => {
                    return Err(SubmitFailure::Conflict {
                        current: memory.revision as i64,
                        expected: format!("expected {side} revision {expected}"),
                    });
                }
                Ok(Some(_)) => {}
                Ok(None) => {
                    return Err(SubmitFailure::Service(MnexisError::not_found(format!(
                        "memory:{uuid}"
                    ))));
                }
                Err(e) => return Err(SubmitFailure::Service(e)),
            }
        }
        Ok(())
    }

    fn required(&self, label: &str) -> Result<String, SubmitFailure> {
        let value = self.value(label).to_string();
        if value.trim().is_empty() {
            return Err(SubmitFailure::Invalid(format!("{label} is required")));
        }
        Ok(value)
    }

    fn revision(&self, label: &str) -> Result<i32, SubmitFailure> {
        self.required(label)?
            .parse::<i32>()
            .map_err(|_| SubmitFailure::Invalid(format!("{label} must be a whole number")))
    }
}

fn short_uuid(uuid: &str) -> String {
    uuid.chars().take(8).collect()
}

fn blank_to_none(value: &str) -> Option<String> {
    (!value.trim().is_empty()).then(|| value.trim().to_string())
}

fn check_bytes(field: &str, value: &str, max_bytes: usize) -> Result<(), SubmitFailure> {
    if value.len() > max_bytes {
        return Err(SubmitFailure::Invalid(format!(
            "{field} exceeds max bytes: {} > {max_bytes}",
            value.len()
        )));
    }
    Ok(())
}

fn check_tags(raw: &str, max_tags: usize) -> Result<(), SubmitFailure> {
    let count = raw
        .split(',')
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .count();
    if count > max_tags {
        return Err(SubmitFailure::Invalid(format!(
            "tags exceeds max count: {count} > {max_tags}"
        )));
    }
    Ok(())
}

fn validate_save_state(raw: &str) -> Result<String, SubmitFailure> {
    let raw = if raw.is_empty() { "active" } else { raw };
    let state = commands::parse_state(raw).map_err(|_| {
        SubmitFailure::Invalid(format!("state must be one of {}", SAVE_STATES.join(", ")))
    })?;
    if !SAVE_STATES.contains(&state.as_str()) {
        return Err(SubmitFailure::Invalid(format!(
            "state must be one of {} (got {})",
            SAVE_STATES.join(", "),
            state.as_str()
        )));
    }
    Ok(state.as_str().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigation_clamps_and_enter_edits_only_multiline_fields() {
        let mut form = Form::save("ns-a");
        assert_eq!(form.focused().unwrap().label, "content");
        assert!(form.enter_edits(), "content is multi-line");
        form.prev();
        assert_eq!(form.index, 0, "prev clamps at the first field");
        form.newline();
        assert_eq!(form.value("content"), "\n");
        for _ in 0..20 {
            form.next();
        }
        assert!(form.is_last(), "next clamps at the last field");
        assert_eq!(form.focused().unwrap().label, "state");
        assert!(!form.enter_edits(), "state is single-line");
        form.backspace();
        assert_eq!(form.value("state"), "activ");
        form.insert('e');
        assert_eq!(form.value("state"), "active");
    }

    #[test]
    fn blank_optionals_become_none_and_server_limits_are_mirrored() {
        assert_eq!(blank_to_none("  "), None);
        assert_eq!(blank_to_none(" k "), Some("k".to_string()));
        assert!(check_bytes("content", "abcd", 3).is_err());
        assert!(check_tags("a, b", 1).is_err());
        assert!(
            check_tags("a,, ,b", 2).is_ok(),
            "blank tags are not counted"
        );
        assert!(
            validate_save_state("bogus").is_err(),
            "unknown states are rejected before the service call"
        );
        assert!(matches!(validate_save_state("proposed"), Ok(s) if s == "proposed"));
    }
}
