/**
 * mnexis for opencode — one file, both plugin models.
 *
 * opencode 1.x (`opencode-ai@1.19.5`) loads a module that exports a function
 * returning a hooks object; opencode 2.x (`@opencode/plugin@2.2.5`) loads a
 * module that default-exports `{ id, setup(ctx) }`. The two models never run
 * the same implementation ("V1 plugin implementations do not run in V2"), so
 * this file carries a default export both of them read:
 *
 *   default { id, setup, server }
 *
 *   - V1 detect-mode accepts any default object with an `id` and calls
 *     `.server(input, options)` (`packages/opencode/src/plugin/shared.ts`).
 *   - V2 decodes `{ id, setup }` and drops the extra member
 *     (`packages/core/src/plugin/module.ts`, an Effect `Schema.Struct`).
 *
 * `Plugin.define` is a runtime passthrough in V2, so nothing is imported from
 * opencode here: the file has no dependency and no `node_modules`.
 * `RESEARCH.md` records the sources and the one measured bet in this shape.
 *
 * What the plugin does — and nothing else:
 *
 * | rule        | why the host must carry it                          |
 * | ----------- | --------------------------------------------------- |
 * | capture     | `user_prompts` has no writer that does not need a model |
 * | session     | a session must land where its memories land          |
 * | pointer     | a session that never hears of the door never asks    |
 * | compaction  | the state must survive the context that held it      |
 * | gate        | permissions belong to the human, not to the agent    |
 * | definitions | the catalog the model reads is the host's (V2 only)  |
 *
 * The last rule is opencode 2 only: `ctx.tool.transform` is the one seat that
 * can rewrite an MCP tool's model-facing definition, and 1.x has no equivalent.
 * It adds no tool and changes no answer — only the words a model reads before
 * it decides to call one.
 *
 * The memory tools themselves are the MCP server's (`mnexis mcp serve`): one
 * shape for every host, never a second copy of the same facts. This file adds
 * only what MCP and the CLI cannot do — the parts the host owns.
 */

import { execFile as nodeExecFile } from "node:child_process";

/** The plugin's stable id, in both models. */
export const PLUGIN_ID = "mnexis";

/** Set to `0` to stop the plugin from pointing at the door. */
export const POINTER_ENV = "MNEXIS_OPENCODE_POINTER";

/** How many live state records the compaction block may carry. */
export const STATE_LIMIT = 8;

/** How many characters of state content the compaction block may carry. */
export const STATE_CHARS = 3000;

/** How much of the previous session's summary the pointer may carry. */
export const SUMMARY_CHARS = 240;

// --- the CLI ---------------------------------------------------------------

/** Default exec: resolves with `{ code, stdout, stderr }` and never rejects. */
export function defaultExec(command, args, options = {}) {
	return new Promise((resolve) => {
		nodeExecFile(
			command,
			args,
			{ encoding: "utf8", maxBuffer: 8 * 1024 * 1024, ...options },
			(error, stdout, stderr) => {
				let code = 0;
				if (error) {
					code = typeof error.code === "number" ? error.code : 1;
					if (error.code === "ENOENT") {
						code = 127;
					}
				}
				resolve({ code, stdout: stdout ?? "", stderr: stderr ?? "" });
			},
		);
	});
}

/** Parse CLI JSON output, tolerating empty or noisy output. */
export function parseJson(stdout) {
	const text = String(stdout ?? "").trim();
	if (text === "") {
		return null;
	}
	try {
		return JSON.parse(text);
	} catch {
		const start = text.indexOf("{");
		const end = text.lastIndexOf("}");
		if (start >= 0 && end > start) {
			try {
				return JSON.parse(text.slice(start, end + 1));
			} catch {
				return null;
			}
		}
		return null;
	}
}

/**
 * Create the CLI client. The same shape the pi extension uses, so the two
 * integrations never disagree about an argument: `exec` is injected by the
 * tests, and neither the binary nor a profile is needed to exercise it.
 */
export function createClient({ exec = defaultExec, binary, env = process.env } = {}) {
	const command = binary || env.MNEXIS_BIN || "mnexis";

	async function run(args, { json = false, cwd } = {}) {
		const fullArgs = json ? [...args, "--json"] : args;
		const result = await exec(command, fullArgs, { cwd, env });
		return { ...result, data: json ? parseJson(result.stdout) : null };
	}

	return {
		command,
		run,

		/** Where this workspace may write, and what is already there. */
		async status(cwd) {
			return run(["status"], { json: true, cwd });
		},

		/** The recall of the session before this one. */
		async previousSession(cwd) {
			return run(["recall", "--previous"], { json: true, cwd });
		},

		/** The live records that must survive a compaction, with their content. */
		async stateRecords(cwd, limit = STATE_LIMIT) {
			return run(
				["list", "--type", "project_state,decision", "--full", "--limit", String(limit)],
				{ json: true, cwd },
			);
		},

		async sessionStart({ sessionId, directory }, cwd) {
			const args = ["session", "--id", sessionId];
			if (directory) {
				args.push("--directory", directory);
			}
			return run(args, { json: true, cwd });
		},

		async sessionEnd({ sessionId }, cwd) {
			return run(["end", "--id", sessionId], { json: true, cwd });
		},

		async savePrompt({ content, sessionId }, cwd) {
			const args = ["prompt", content];
			if (sessionId) {
				args.push("--session", sessionId);
			}
			return run(args, { json: true, cwd });
		},
	};
}

// --- pure decisions (every one of them is pinned by a test) ----------------

/**
 * The commands and file writes that would grant write permissions without
 * the human. The plugin owns that rule on every host, so a model-issued call
 * that tries to do it behind the plugin's back is refused.
 *
 * The pattern is the pi extension's, unchanged: the same grant means the same
 * thing in both hosts, and two copies of the rule that disagree would be a
 * hole in one of them.
 */
const PERMISSION_GRANT_PATTERN =
	/\bmnexis\s+config\s+allow-namespace\b|\bmnexis\s+project\s+bind\b/;

/** The profile files a model must never edit by hand. */
const PROFILE_FILE_PATTERN = /(?:^|[\\/])(mnexis|projects)\.toml$/;

/** The shell tools: V1 calls it `bash`, V2 calls it `shell`. */
const SHELL_TOOLS = new Set(["bash", "shell"]);

/** The file-writing tools: V2 adds `patch`. */
const EDIT_TOOLS = new Set(["edit", "write", "patch"]);

/** What a blocked grant says. opencode cannot ask the human from a plugin. */
export const GRANT_REFUSAL =
	"Granting a namespace is the human's decision, and this host cannot ask them from " +
	"inside a plugin: nothing was granted. Ask the human to run " +
	"`mnexis config allow-namespace <namespace>` once — the grant persists in the " +
	"profile and the retry works — or `mnexis project bind <namespace> --key <key>` " +
	"for a workspace with no namespace yet.";

/** What a blocked profile edit says. */
export const PROFILE_REFUSAL =
	"The mnexis profile files are managed by the mnexis CLI, never edited by hand. " +
	"Use `mnexis init`, `mnexis project bind` or `mnexis config allow-namespace` instead.";

/**
 * The refusal a tool call earns, or `null` when it is the model's to make.
 *
 * @param {string} tool the host's tool id
 * @param {unknown} input the arguments the model passed
 * @returns {string | null}
 */
export function gateViolation(tool, input) {
	const name = typeof tool === "string" ? tool : "";
	if (SHELL_TOOLS.has(name)) {
		const command = typeof input?.command === "string" ? input.command : "";
		return PERMISSION_GRANT_PATTERN.test(command) ? GRANT_REFUSAL : null;
	}
	if (EDIT_TOOLS.has(name)) {
		// V1 and V2 name the target `filePath`; a tool that names it `path`
		// is checked too rather than trusted.
		const target =
			typeof input?.filePath === "string"
				? input.filePath
				: typeof input?.path === "string"
					? input.path
					: "";
		return PROFILE_FILE_PATTERN.test(target) ? PROFILE_REFUSAL : null;
	}
	return null;
}

/**
 * The text a submitted user message carries, from its parts (V1's
 * `chat.message` output).
 *
 * A part the host wrote is `synthetic: true` — an agent-part hint, an
 * auto-continue — and is not something the person said. A message whose every
 * text part is synthetic has nothing to capture.
 *
 * @param {unknown} parts
 * @returns {string}
 */
export function promptTextFromParts(parts) {
	if (!Array.isArray(parts)) {
		return "";
	}
	const texts = [];
	for (const part of parts) {
		if (!part || typeof part !== "object") continue;
		if (part.type !== "text") continue;
		if (part.synthetic === true) continue;
		if (typeof part.text !== "string") continue;
		texts.push(part.text);
	}
	return texts.join("\n").trim() === "" ? "" : texts.join("\n");
}

/** True when the host's message is not something the person typed. */
export function isHumanMessage(message) {
	if (!message || typeof message !== "object") {
		return true;
	}
	return message.role === undefined || message.role === "user";
}

/**
 * The session id an event names, in either host's shape.
 *
 * V1: `{ type: "session.created", properties: { info: { id } } }` for the events
 * that name a whole session, and `{ type: "session.idle", properties: {
 * sessionID } }` for the ones that name it by id.
 * V2: `{ type: "session.created", data: { sessionID } }`
 *
 * @param {unknown} event
 * @returns {string | null}
 */
export function sessionIDOfEvent(event) {
	if (!event || typeof event !== "object") {
		return null;
	}
	const fromData = event.data && typeof event.data.sessionID === "string" ? event.data.sessionID : null;
	if (fromData) {
		return fromData;
	}
	const properties = event.properties && typeof event.properties === "object" ? event.properties : null;
	if (properties && typeof properties.sessionID === "string" && properties.sessionID !== "") {
		return properties.sessionID;
	}
	const info = properties && properties.info;
	if (info && typeof info.id === "string" && info.id !== "") {
		return info.id;
	}
	return null;
}

/** The directory an event names, when it names one. */
export function directoryOfEvent(event) {
	if (!event || typeof event !== "object") {
		return null;
	}
	const info = event.properties && event.properties.info;
	if (info && typeof info.directory === "string" && info.directory !== "") {
		return info.directory;
	}
	const location = event.location;
	if (location && typeof location.directory === "string" && location.directory !== "") {
		return location.directory;
	}
	return null;
}

/**
 * Render the engine's live state records as the block a compaction keeps.
 *
 * The selection is the engine's, never the plugin's: `list` already answers
 * the live `project_state` and `decision` records of this workspace's
 * namespace, newest first, and `--full` gives their whole content. The block
 * carries as many as fit its budget and says how many did not.
 *
 * @param {unknown} listData the `list --json` payload
 * @returns {string} the block, or "" when there is nothing to carry
 */
export function stateBlockText(listData) {
	const items = listData && typeof listData === "object" && Array.isArray(listData.items)
		? listData.items
		: [];
	const lines = [
		"mnexis — the live state of this project, which the summary above does not replace " +
			"(from the memory engine, newest first):",
	];
	let used = lines[0].length;
	let carried = 0;
	let kept = 0;
	for (const item of items) {
		if (!item || typeof item !== "object") continue;
		const content = typeof item.content === "string" ? item.content.trim() : "";
		if (content === "") continue;
		kept += 1;
		const type = typeof item.memory_type === "string" && item.memory_type !== "" ? item.memory_type : "fact";
		const title = typeof item.title === "string" && item.title !== "" ? item.title : "(untitled)";
		const uuid = typeof item.uuid === "string" ? item.uuid : "?";
		const revision = Number.isFinite(item.revision) ? `, rev ${item.revision}` : "";
		const head = `\n- [${type}] ${title} (${uuid}${revision})`;
		const body = `\n  ${content.split("\n").join("\n  ")}`;
		if (used + head.length + body.length > STATE_CHARS) continue;
		lines.push(head + body);
		used += head.length + body.length;
		carried += 1;
	}
	if (carried === 0) {
		return "";
	}
	const left = kept - carried;
	if (left > 0) {
		lines.push(
			`\n… ${left} more live record${left === 1 ? "" : "s"} did not fit; ` +
				"read them with `mnexis show <uuid>`.",
		);
	}
	return lines.join("");
}

/**
 * The one-shot pointer to the door.
 *
 * It names the namespace, the door and the rules that make the door cheap —
 * and it does **not** re-render the memories or the state block: the board
 * measured that an injected state block did not remove the verification
 * round (`odd/next.md` §11), so a second projection would buy tokens and
 * nothing else.
 *
 * @param {{ status?: unknown, previous?: unknown }} input
 *   `status`: the `status --json` payload; `previous`: the `recall
 *   --previous --json` payload.
 * @returns {string} the pointer, or "" when the workspace names no namespace
 */
export function pointerText({ status, previous } = {}) {
	const workspace = status && typeof status === "object" ? status.workspace : null;
	const namespace =
		workspace && typeof workspace.namespace === "string" && workspace.namespace !== ""
			? workspace.namespace
			: null;
	// A workspace with no binding has no namespace to name, and the plugin
	// never invents one: the skill teaches the binding, the human decides it.
	if (!namespace) {
		return "";
	}
	const count =
		status && typeof status === "object" && Number.isFinite(status.memory_count)
			? status.memory_count
			: null;
	const lines = [
		`mnexis memory: this workspace writes into namespace \`${namespace}\`` +
			`${count === null ? "" : ` (${count} memories)`}.`,
		"Call the door — `mnexis_memory_context`, or `tools.mnexis.memory_context` under " +
			"Code Mode — with the task in one line before you read or change code: it answers " +
			"the matching memories, the live project state and the previous session's summary.",
	];
	const summary = previousSummary(previous);
	if (summary) {
		lines.push(`Previous session (${summary.when}): ${summary.text}`);
	}
	lines.push(
		"What the user says is already stored verbatim: never copy the conversation into " +
			"`memory_save_prompt`.",
	);
	return lines.join("\n");
}

/** The previous session's summary, as one bounded line. */
function previousSummary(previous) {
	const session = previous && typeof previous === "object" ? previous.session : null;
	if (!session || typeof session !== "object") {
		return null;
	}
	const raw = typeof session.summary === "string" ? session.summary.trim() : "";
	if (raw === "") {
		return null;
	}
	const firstLine = raw.split("\n")[0].trim();
	const text =
		firstLine.length > SUMMARY_CHARS ? `${firstLine.slice(0, SUMMARY_CHARS - 1)}…` : firstLine;
	const started = typeof session.started_at === "string" ? session.started_at : "";
	const when = started.length >= 10 ? started.slice(0, 10) : "date unknown";
	return { when, text };
}

// --- the curated definitions (odd/tasks/opencode-polish.md, S2-S5) --------
//
// opencode lists an MCP server's tools as `<server>_<tool>`, model-facing, with
// the description the engine publishes. The engine's descriptions say what a
// tool *does*; the curated ones below say what it is *for*, in pi's voice, so a
// model on opencode meets the vocabulary a model on pi meets. This is an inline
// copy on purpose — the file has no dependency and no `node_modules` (S6) — and
// a test pins every entry to the files it was copied from (S5).

/** The prefix the host gives every tool of the `mnexis` MCP server. */
const MCP_TOOL_PREFIX = "mnexis_";

/**
 * The seven kinds, one line each — the same lines pi's `SETTLE_NUDGE` carries
 * (`extensions/mnexis-memory/index.js`). One definition of the vocabulary, two
 * consumers: pi's nudge and the save description below. A test pins the two
 * copies together, verbatim.
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

/** What the save description states before the seven kinds it lets you pick. */
const SAVE_GLOSS_HEADER = "Save it typed — `memory_type` is the one that fits:";

/**
 * The model-facing description of every tool the engine's MCP server publishes
 * (`src/mcp/server.rs`), keyed by the id opencode lists. One entry per engine
 * tool, no more: a test pins the map to the engine's inventory, so a renamed or
 * added tool fails the build instead of shipping a dead entry.
 *
 * The wording is pi's where pi has the same tool, adapted to what the MCP
 * surface actually answers (previews, ids, and the fields the tool takes).
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

// --- the behaviour every host shares --------------------------------------

/**
 * The plugin's engine-facing behaviour, shared by both plugin models.
 *
 * Every method is best effort: a host hook must never fail a turn because the
 * memory engine could not be reached, and it must never fail silently either
 * — a refusal reaches `logger`, which each adapter wires to whatever the host
 * offers (V1 has `client.app.log`; V2 has no logger in the plugin context).
 */
function createCore({ cwd, client, env = process.env, logger } = {}) {
	const cli = client ?? createClient({ env });
	const sessions = new Map();
	const pointed = new Set();

	function report(kind, data) {
		if (typeof logger !== "function") {
			return;
		}
		try {
			logger({ kind, ...data });
		} catch {
			// The journal is a report, never a precondition.
		}
	}

	/** Register a session once, in the directory the host gave. */
	async function startSession(sessionID, directory) {
		if (typeof sessionID !== "string" || sessionID === "") {
			return;
		}
		if (sessions.has(sessionID)) {
			return;
		}
		sessions.set(sessionID, directory || cwd);
		try {
			await cli.sessionStart({ sessionId: sessionID, directory: directory || cwd }, cwd);
		} catch (error) {
			// A profile that is not initialised, or a namespace not allowed
			// for writes, is the engine's answer; the session still works.
			report("session-start", { session: sessionID, error: String(error) });
		}
	}

	/** End every session this plugin opened, best effort. */
	async function endSessions() {
		for (const [sessionID, directory] of sessions) {
			try {
				await cli.sessionEnd({ sessionId: sessionID }, directory);
			} catch (error) {
				report("session-end", { session: sessionID, error: String(error) });
			}
		}
		sessions.clear();
	}

	/**
	 * Store what the person typed, byte for byte.
	 *
	 * The session is registered first, so the prompt joins the session and
	 * the memories of one workspace. A message the store refuses is never
	 * truncated and never lost silently: its size and exit code reach the
	 * journal, and its text never does.
	 */
	async function capture(sessionID, text) {
		if (typeof text !== "string" || text.trim() === "") {
			return;
		}
		await startSession(sessionID, cwd);
		const bytes = Buffer.byteLength(text, "utf8");
		try {
			const result = await cli.savePrompt({ content: text, sessionId: sessionID }, cwd);
			if (result.code !== 0) {
				report("capture", { session: sessionID, bytes, exit_code: result.code });
			}
		} catch (error) {
			report("capture", { session: sessionID, bytes, error: String(error) });
		}
	}

	/**
	 * The pointer for this session, delivered once.
	 *
	 * A failed read is remembered as delivered too: an engine that cannot
	 * answer must not cost a CLI call on every model request.
	 */
	async function pointer(sessionID) {
		if (env[POINTER_ENV] === "0") {
			return null;
		}
		// A request the host did not name belongs to no session, so there is
		// no "once" to count against it: the pointer waits for a real one.
		if (typeof sessionID !== "string" || sessionID === "") {
			return null;
		}
		if (pointed.has(sessionID)) {
			return null;
		}
		pointed.add(sessionID);
		try {
			const status = await cli.status(cwd);
			if (status.code !== 0) {
				report("pointer", { session: sessionID, exit_code: status.code });
				return null;
			}
			let previous = null;
			try {
				const recall = await cli.previousSession(cwd);
				if (recall.code === 0) {
					previous = recall.data;
				}
			} catch {
				// No previous session is not an error: the pointer is shorter.
			}
			const text = pointerText({ status: status.data, previous });
			return text === "" ? null : text;
		} catch (error) {
			report("pointer", { session: sessionID, error: String(error) });
			return null;
		}
	}

	/** The block a compaction keeps: the engine's own live state. */
	async function compaction() {
		try {
			const result = await cli.stateRecords(cwd, STATE_LIMIT);
			if (result.code !== 0) {
				report("compaction", { exit_code: result.code });
				return null;
			}
			const text = stateBlockText(result.data);
			return text === "" ? null : text;
		} catch (error) {
			report("compaction", { error: String(error) });
			return null;
		}
	}

	return { cli, capture, startSession, endSessions, pointer, compaction };
}

// --- the plugin, in both shapes -------------------------------------------

/**
 * Build the plugin.
 *
 * `exec`/`binary`/`env`/`logger` are injectable so the tests never need the
 * binary, a profile or a host.
 */
export function createPlugin({ exec, binary, env = process.env, logger } = {}) {
	const coreFor = (cwd, hostLogger) =>
		createCore({
			cwd,
			env,
			client: createClient({ exec, binary, env }),
			logger: logger ?? hostLogger,
		});

	/** opencode 2.x: `setup(ctx)` registers hooks on the domains that own them. */
	async function setup(ctx = {}) {
		const cwd = typeof ctx.location?.directory === "string" ? ctx.location.directory : process.cwd();
		const core = coreFor(cwd, undefined);
		const controllers = [];

		// Each seat is registered on its own: a host that does not have one
		// (an older 2.x without `compaction`, a test double, a future rename)
		// must still get the rest, rather than lose the whole plugin — the
		// capture, above all, is the rule nothing else provides.
		if (ctx.session && typeof ctx.session.hook === "function") {
			// The prompt hook runs once, at admission, for what a person
			// submitted — the host already excludes synthetic messages, shell
			// messages and compaction controls, so no filter is needed here.
			try {
				await ctx.session.hook("prompt", async (event) => {
					await core.capture(event?.sessionID, event?.prompt?.text);
				});
			} catch {
				// No prompt seat: the other rules still run.
			}

			// The system prompt is assembled per model request; the pointer is
			// delivered once per session and is a no-op afterwards.
			try {
				await ctx.session.hook("context", async (event) => {
					const text = await core.pointer(event?.sessionID);
					if (text && Array.isArray(event?.system)) {
						event.system.push({ type: "text", text });
					}
				});
			} catch {
				// No context seat: the pointer simply is not delivered.
			}

			// Compaction keeps the live state the transcript was carrying.
			try {
				await ctx.session.hook("compaction", async (event) => {
					const block = await core.compaction();
					if (block && Array.isArray(event?.system)) {
						event.system.push({ type: "text", text: block });
					}
				});
			} catch {
				// No compaction seat: the summary is the host's own.
			}
		}

		if (ctx.tool && typeof ctx.tool.hook === "function") {
			try {
				await ctx.tool.hook("execute.before", (event) => {
					const refusal = gateViolation(event?.tool, event?.input);
					if (refusal) {
						throw new Error(refusal);
					}
				});
			} catch {
				// No tool seat: nothing is guarded, and nothing else is lost.
			}
		}

		if (ctx.tool && typeof ctx.tool.transform === "function") {
			try {
				// The catalog the model reads, rewritten in the seat the host
				// re-runs on every refresh. The whole body is wrapped: a measured
				// edge of this seat is that an uncaught throw inside the callback
				// corrupts the turn, so a plugin must never let one out.
				await ctx.tool.transform((editor) => {
					try {
						curateCatalog(editor);
					} catch {
						// The catalog the host built is the catalog the model keeps.
					}
				});
			} catch {
				// No catalog seat: the tools keep the engine's own definitions.
			}
		}

		if (ctx.event && typeof ctx.event.subscribe === "function") {
			try {
				const controller = new AbortController();
				controllers.push(controller);
				const stream = ctx.event.subscribe({ types: ["session.created"], signal: controller.signal });
				void (async () => {
					try {
						for await (const event of stream) {
							const sessionID = sessionIDOfEvent(event);
							await core.startSession(sessionID, directoryOfEvent(event) ?? cwd);
						}
					} catch {
						// The stream ends when the plugin unloads, and a session
						// the engine never heard of is not an error.
					}
				})();
			} catch {
				// No event stream: a session is still registered by its first
				// prompt (see `capture`).
			}
		}

		return async () => {
			for (const controller of controllers) {
				controller.abort();
			}
			await core.endSessions();
		};
	}

	/** opencode 1.x: the plugin function returns one hooks object. */
	async function server(input = {}) {
		const cwd =
			typeof input.directory === "string"
				? input.directory
				: typeof input.worktree === "string"
					? input.worktree
					: process.cwd();
		const hostLogger =
			input.client && input.client.app && typeof input.client.app.log === "function"
				? (record) =>
						void input.client.app.log({
							body: { service: PLUGIN_ID, level: "warn", message: "mnexis", extra: record },
						})
				: undefined;
		const core = coreFor(cwd, hostLogger);

		return {
			// The human's message, captured where it is said. A prompt a
			// plugin sent is `synthetic`, exactly like the host's own.
			async "chat.message"(hookInput, output) {
				if (!isHumanMessage(output?.message)) {
					return;
				}
				await core.capture(hookInput?.sessionID, promptTextFromParts(output?.parts));
			},

			async event({ event }) {
				if (!event || event.type !== "session.created") {
					return;
				}
				await core.startSession(sessionIDOfEvent(event), directoryOfEvent(event) ?? cwd);
			},

			async "tool.execute.before"(hookInput, output) {
				const refusal = gateViolation(hookInput?.tool, output?.args);
				if (refusal) {
					throw new Error(refusal);
				}
			},

			// The system prompt is assembled per model request; the pointer is
			// delivered once per session and is a no-op afterwards.
			async "experimental.chat.system.transform"(hookInput, output) {
				const text = await core.pointer(hookInput?.sessionID);
				if (text && Array.isArray(output?.system)) {
					output.system.push(text);
				}
			},

			async "experimental.session.compacting"(_hookInput, output) {
				const block = await core.compaction();
				if (block && Array.isArray(output?.context)) {
					output.context.push(block);
				}
			},

			async dispose() {
				await core.endSessions();
			},
		};
	}

	return { id: PLUGIN_ID, setup, server };
}

/**
 * The plugin both hosts load.
 *
 * V1 detect-mode reads `id` and calls `server`; V2 decodes `{ id, setup }` and
 * drops the rest. The tests build their own through `createPlugin`, so they can
 * inject the exec that the member above does not.
 */
const plugin = createPlugin();

export default plugin;
