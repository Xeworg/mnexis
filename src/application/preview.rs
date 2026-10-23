//! The compact projection of a memory: what is there, not what it says.
//!
//! A list is a list. An agent that asks "what does this project already
//! hold?" needs the identity of every memory and a preview of its content;
//! it does not need the whole text of twenty memories to decide which one
//! to open. The benchmark measured the difference: one `mnexis list` call
//! with the full content of five memories cost 9.3 KB of context for
//! nothing.
//!
//! The complete record stays one call away (`mnexis show <uuid>`), and
//! `--full` asks for the whole shape explicitly. Nothing is truncated
//! silently: the preview ends with an ellipsis when it was cut.

use serde::Serialize;

use crate::domain::memory::Memory;

/// How many characters of the content a preview holds.
pub const PREVIEW_CHARS: usize = 160;

/// The compact view of one memory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MemoryPreview {
    /// The memory's stable identifier, as `mnexis show` expects it.
    pub uuid: String,
    /// The namespace the memory lives in.
    pub namespace: String,
    /// The memory's type (`fact`, `decision`, …).
    pub memory_type: String,
    /// The memory's lifecycle state.
    pub state: String,
    /// The memory's current revision.
    pub revision: i32,
    /// The topic the memory belongs to, when it has one.
    pub topic_key: String,
    /// When the memory was created.
    pub created_at: String,
    /// When the memory was last written.
    pub updated_at: String,
    /// The first words of the content. It ends with `…` when it was cut.
    pub preview: String,
}

impl MemoryPreview {
    /// The compact view of a memory.
    pub fn of(memory: &Memory) -> Self {
        Self {
            uuid: memory.uuid.clone(),
            namespace: memory.namespace.clone(),
            memory_type: memory.memory_type.clone(),
            state: memory.state.as_str().to_string(),
            revision: memory.revision,
            topic_key: memory.topic_key.clone(),
            created_at: memory.created_at.clone(),
            updated_at: memory.updated_at.clone(),
            preview: preview(&memory.content),
        }
    }
}

/// The compact views of a list of memories.
pub fn previews(memories: &[Memory]) -> Vec<MemoryPreview> {
    memories.iter().map(MemoryPreview::of).collect()
}

/// The first words of a content, with an ellipsis when it was cut.
fn preview(content: &str) -> String {
    let mut out: String = content.chars().take(PREVIEW_CHARS).collect();
    if content.chars().count() > PREVIEW_CHARS {
        out.push('…');
    }
    out.replace('\n', " ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::memory::{Memory, MemoryState};

    fn memory_with(content: &str) -> Memory {
        Memory {
            id: Some(1),
            uuid: "uuid-1".to_string(),
            title: String::new(),
            topic_key: "topic-1".to_string(),
            namespace: "ns".to_string(),
            memory_type: "fact".to_string(),
            content: content.to_string(),
            state: MemoryState::Active,
            revision: 3,
            deleted: false,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-02T00:00:00Z".to_string(),
            deleted_at: None,
            purge_after: None,
            valid_from: None,
            valid_until: None,
            actor: "user".to_string(),
            channel: "cli".to_string(),
            assertion_basis: "user_stated".to_string(),
        }
    }

    #[test]
    fn a_preview_carries_the_identity_and_a_glimpse() {
        let preview = MemoryPreview::of(&memory_with("El servicio escucha en el puerto 8081"));
        assert_eq!(preview.uuid, "uuid-1");
        assert_eq!(preview.namespace, "ns");
        assert_eq!(preview.revision, 3);
        assert_eq!(preview.topic_key, "topic-1");
        assert_eq!(preview.preview, "El servicio escucha en el puerto 8081");
    }

    #[test]
    fn a_long_content_is_cut_and_says_so() {
        let long = "x".repeat(PREVIEW_CHARS + 50);
        let preview = MemoryPreview::of(&memory_with(&long));
        assert!(preview.preview.ends_with('…'), "the cut is marked");
        assert_eq!(preview.preview.chars().count(), PREVIEW_CHARS + 1);
    }

    #[test]
    fn a_preview_is_smaller_than_the_content_it_summarises() {
        let long = "una frase bastante larga que se repite. ".repeat(40);
        let memory = memory_with(&long);
        let preview = MemoryPreview::of(&memory);
        let compact = serde_json::to_string(&preview).unwrap();
        let full = serde_json::to_string(&memory).unwrap();
        assert!(
            compact.len() * 4 < full.len(),
            "the compact shape must be much smaller: {} vs {}",
            compact.len(),
            full.len()
        );
    }

    #[test]
    fn a_preview_has_one_line() {
        let preview = MemoryPreview::of(&memory_with("first line\nsecond line"));
        assert_eq!(preview.preview, "first line second line");
    }
}
