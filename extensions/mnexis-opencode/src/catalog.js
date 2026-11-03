// --- the curated definitions (odd/tasks/opencode-polish.md, S2-S5) --------
//
// opencode lists an MCP server's tools as `<server>_<tool>`, model-facing, with
// the description the engine publishes. The engine's descriptions say what a
// tool *does*; the curated ones below say what it is *for*, in this host's
// voice, so a model on opencode meets the vocabulary the engine defines. This
// is an inline copy on purpose — the file has no dependency and no
// `node_modules` (S6) — and a test pins every entry to the engine's inventory
// (S5).

/** The prefix the host gives every tool of the `mnexis` MCP server. */
const MCP_TOOL_PREFIX = "mnexis_";

/**
 * The seven kinds, one line each — this plugin's own copy of the vocabulary.
 *
 * The NAMES are the engine's closed set: `MEMORY_TYPES` in
 * `src/domain/memory_type.rs`, stated in the `memory_type` docs of
 * `src/mcp/tools.rs` that schemars carries into the MCP schema every host
 * reads. The engine is the only contract the hosts share, and a test pins
 * these names to it. The gloss is this host's: each surface owns its wording
 * and may evolve it without touching another surface.
 */
export const TYPE_GLOSSES = [
	"knowledge — a durable fact about the domain, the tools or the data.",
	"decision — a choice made, and what it beat.",
	"preference — how the user wants things done.",
	"procedure — the steps that reliably achieve something.",
	"project_state — where the work stands now.",
	"episode — something that happened in a session.",
	"fact — the neutral default when none of the others fit.",
];

/** The seven names, split from the glosses: the plugin states them once. */
const TYPE_NAMES = TYPE_GLOSSES.map((gloss) => gloss.split(" — ")[0]);

/**
 * The question the settle nudge asks, once per session.
 *
 * The hooks capture the mechanical floor; whether a session deserved a memory
 * is judgment. The plugin asks it when the session's own outcomes show work
 * that may have changed a file and no `mnexis_memory_save` that landed, and
 * the ask points at the taxonomy's home: the save tool, and the seven
 * `memory_type` names it accepts. It asks for the judgment, not for a quota.
 */
export const SETTLE_NUDGE = [
	"This session changed files but saved no memory. If a decision, preference, " +
		"discovery or project-state change deserves to survive the session, save it now " +
		"with one `mnexis_memory_save`; otherwise reply that nothing deserves memory.",
	"",
	"`mnexis_memory_save` carries the vocabulary: `memory_type` is the engine's closed " +
		"set — " +
		TYPE_NAMES.join(", ") +
		" — so pick the one that fits.",
].join("\n");

/** What the save description states before the seven kinds it lets you pick. */
const SAVE_GLOSS_HEADER = "Save it typed — `memory_type` is the one that fits:";

/**
 * The model-facing description of every tool the engine's MCP server publishes
 * (`src/mcp/server.rs`), keyed by the id opencode lists. One entry per engine
 * tool, no more: a test pins the map to the engine's inventory, so a renamed or
 * added tool fails the build instead of shipping a dead entry.
 *
 * The wording is this host's own: it follows what the MCP surface actually
 * answers (previews, ids, and the fields the tool takes).
 */
export const CURATED_DESCRIPTIONS = {
	mnexis_memory_context:
		"Call this FIRST when you are about to work: give it the task in one line and it " +
		"answers the context in one call — the memories that match the task, best first, " +
		"each with its identity and a preview of at most 160 characters (open one with " +
		"`mnexis_memory_get` when it is the one you need), the session state of this " +
		"project (the live project_state and decision records that fit the block's own " +
		"budget, the index of the records that did not fit, and the previous session's " +
		"summary), and what the answer did not include: how many candidates the budget " +
		"could not keep. It reads this workspace's namespace — the one a save lands in — " +
		"and the answer names the namespace it searched. `mnexis_memory_search` is the " +
		"exact query.",
	mnexis_memory_delete:
		"Soft-delete a memory: it leaves the live records and the search index, and its " +
		"history keeps it. `mnexis_memory_restore` brings it back.",
	mnexis_memory_get:
		"Read one memory whole, by its UUID: its content, its current revision and its " +
		"state. Every other read answers previews; this is the record itself.",
	mnexis_memory_history:
		"Return one memory's revision history: every revision in order with its state, " +
		"content, who changed it and why. Use it before updating or superseding the fact.",
	mnexis_memory_list:
		"List the memories this project already holds, with a preview of each. Reaches " +
		"every namespace you may read; `namespace` narrows it. Read one with " +
		"`mnexis_memory_get`; `full` gives every memory whole.",
	mnexis_memory_recall:
		"Recall a session: the session, what the user said in order, and the namespace's " +
		"memories in its window. `previous` is this workspace namespace's most recent " +
		"finished session; `session` takes an id. Memories are read by time, not by " +
		"session, each with identity and a preview; `full` gives whole records. The " +
		"answer also carries the branch state the session was anchored to, checked at " +
		"read time and never stored: `merged`, `not_merged` or `unknown`.",
	mnexis_memory_restore:
		"Restore a soft-deleted memory: it rejoins the live records and the search " +
		"index, with its history intact.",
	mnexis_memory_save: [
		"Store a durable memory of this project: content must stand alone months later, " +
			"and the namespace is this workspace's. The title is what the fact IS and it " +
			"picks the record — the same title always lands on the same record, so a " +
			"changed fact updates it. Keywords are other languages' or teams' names for " +
			"it, never a value.",
		"",
		SAVE_GLOSS_HEADER,
		...TYPE_GLOSSES,
	].join("\n"),
	mnexis_memory_save_prompt:
		"Store a message verbatim for the one case automatic capture misses: something " +
		"pasted from another conversation, or a statement that must be found by its exact " +
		"words. This session's messages are already stored, so do not copy the " +
		"conversation here.",
	mnexis_memory_search:
		"Search the stored memories (full-text, ranked) before answering from scratch or " +
		"changing existing code. Reaches every namespace you may read; `namespace` narrows " +
		"it. Write the query as you would say it: every word counts, and a hyphen is part " +
		"of a word (`staging-eu`). Each hit is identity and kind — open one with " +
		"`mnexis_memory_get`.",
	mnexis_memory_search_prompts:
		"Search what the user said before, across sessions: the request behind the work. " +
		"Write the query as you would say it: every word counts, and a hyphen is part of a " +
		"word.",
	mnexis_memory_session_end:
		"End your session with a short summary of what it did. The next session reads it " +
		"first, so it must stand alone.",
	mnexis_memory_session_start:
		"Register your session, so the engine knows its boundaries. Call it once with the " +
		"session id you were given — never invented; this plugin already registers the " +
		"sessions the host opens. The engine never ends a session for you.",
	mnexis_memory_suggest_topic_key:
		"Ask which topic key a title yields and whether this project records it. Use it " +
		"before saving an unsure fact: the answer names the record the title would update.",
	mnexis_memory_supersede:
		"Replace an outdated memory with a newer one, keeping the history. Use it when a " +
		"fact changed: save the new one first, then link both here with the revision each " +
		"one is at. Both must live in this project's namespace; another project's memory " +
		"is refused.",
	mnexis_memory_update:
		"Update a memory's content, kind, state or tags, and say why. It takes the revision " +
		"you believe is current, so two writers cannot overwrite each other silently; a " +
		"`memory_type` you name replaces the record's kind, and omitting it keeps the one " +
		"it has.",
	mnexis_memory_workspace:
		"Call this FIRST, at the start of every session and before changing code: the " +
		"namespace this workspace writes into, the read and write allowlists, how many " +
		"memories the scope can see, and the most recent — title and preview, never the " +
		"whole record (open one with `mnexis_memory_get`). The decisions live here, not in " +
		"the files.",
};

/**
 * Rewrite the model-facing definitions of the mnexis tools in the host's
 * catalog: the curated description, and Code Mode off (opencode 2 hides MCP
 * tools behind Code Mode unless the server says otherwise, and the host's
 * rewrite is the half the engine's own `"codemode": false` cannot reach when
 * the entry was written by an older mnexis).
 *
 * An id the map does not know (the engine renamed a tool) is skipped rather
 * than guessed at; every mnexis id still gets Code Mode off, so the tool stays
 * one direct call. A catalog with no mnexis tools changes nothing.
 *
 * @param {unknown} editor the host's ToolEditor
 */
function curateCatalog(editor) {
	const list = editor && typeof editor.list === "function" ? editor.list() : [];
	if (!Array.isArray(list)) {
		return;
	}
	for (const tool of list) {
		const id = tool && typeof tool.id === "string" ? tool.id : "";
		if (!id.startsWith(MCP_TOOL_PREFIX)) {
			continue;
		}
		const description = CURATED_DESCRIPTIONS[id];
		editor.update(id, (target) => {
			if (typeof description === "string") {
				target.description = description;
			}
			target.options = { ...(target.options ?? {}), codemode: false };
		});
	}
}
