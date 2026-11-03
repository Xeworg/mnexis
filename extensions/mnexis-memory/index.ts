/**
 * mnexis-memory — pi extension exposing the mnexis memory engine.
 *
 * Why an extension and not only a skill: a skill is advisory, so the model
 * decides whether to use it and which namespace to write into. These tools
 * are declared to the model in every session, the workspace → namespace
 * binding is resolved by the extension, and the write permission gate lives
 * in code instead of in prose.
 *
 * The gate asks the human exactly once per namespace. After that the agent
 * saves on its own, inside the namespaces the human already authorized.
 */

import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";

import { createChildRegistry, createClient, defaultExec } from "./lib/cli.ts";
import {
	MEMORY_PROBE_BUDGET_MS,
	PROMPT_CAPTURE_BUDGET_MS,
	SHUTDOWN_BUDGET_MS,
	STATUS_CARD_BUDGET_MS,
	budgetOf,
	createDeadline,
} from "./lib/budget.ts";
import {
	DIALOG_TIMEOUT_REASON,
	askConfirm,
	authorizationPendingMessage,
	decideAuthorization,
	decideBinding,
	describeFailure,
	dialogTimeoutMs,
	engineSilence,
	allowNamespaceCommand,
	bindCommand,
} from "./lib/gate.ts";
import {
	candidateFromMarker,
	deriveWorkspaceCandidate,
	findMarkerNamespace,
	readWorkspaceCandidate,
	selectNamespace,
	workspaceKey,
} from "./lib/workspace.ts";
import { renderers, withStatus } from "./lib/chrome.ts";
import {
	JOURNAL_KINDS,
	counterLine,
	createFooter,
	journalRenderer,
	nudgeRenderer,
} from "./lib/chrome.ts";

/**
 * Register a memory tool with the chrome pi draws: the call line, the panel
 * that names the answer, and the status the footer carries while it runs.
 *
 * The tool's own behaviour is untouched: the text the model reads is exactly
 * what the CLI answered, and the drawing is pi's, on the human's side.
 */
function registerMemoryTool(pi, definition, footer = null) {
	pi.registerTool({
		...definition,
		...renderers(definition.name),
		execute: withStatus(definition.name, definition.execute, footer),
	});
}

/**
 * Register the entry renderers the journal needs (UX1), guarded for older
 * hosts: an entry with no renderer draws nothing, and a host without the
 * renderer API still loads the tools.
 */
function registerJournalRenderers(pi) {
	if (typeof pi?.registerEntryRenderer !== "function") {
		return;
	}
	for (const kind of JOURNAL_KINDS) {
		try {
			pi.registerEntryRenderer(kind, journalRenderer(kind));
		} catch {
			// A host that refuses one renderer still gets the tools.
		}
	}
}

/**
 * Register the renderer the settle nudge needs (UX4), guarded the same way:
 * the message is `display: true`, so a host without renderers shows it with
 * pi's own custom-message shell instead of not showing it at all.
 */
function registerNudgeRenderer(pi) {
	if (typeof pi?.registerMessageRenderer !== "function") {
		return;
	}
	try {
		pi.registerMessageRenderer("mnexis-settle-nudge", nudgeRenderer());
	} catch {
		// The nudge is still sent; only its drawing is missing.
	}
}

/**
 * Which footer counter a call moves (UX2).
 *
 * The door and the workspace answer "what does this project know": they are
 * the `ctx` counter. Search and show are the reads that end in a record, and
 * they are `search`. Saves are counted from their outcome (AB6), not here.
 */
const FOOTER_COLUMNS = new Map([
	["mnexis_context", "ctx"],
	["mnexis_workspace", "ctx"],
	["mnexis_search", "search"],
	["mnexis_show", "search"],
]);

/** The prompt section a bound workspace with memories adds (AB3). */
const MEMORY_SECTION_KEY = "mnexis-memory";

/** Read the write allowlist out of `mnexis config show --json`. */
export function readWriteNamespaces(configData) {
	const scope = configData && typeof configData === "object" ? configData.scope : null;
	const namespaces = scope && typeof scope === "object" ? scope.write_namespaces : null;
	return Array.isArray(namespaces) ? namespaces : [];
}

/**
 * Extract the records from a `--json` payload.
 *
 * `mnexis list --json` and `mnexis search --json` answer with an envelope
 * `{ count, items }`, not with a bare array.
 */
export function readItems(payload) {
	if (Array.isArray(payload)) {
		return payload;
	}
	if (payload && typeof payload === "object" && Array.isArray(payload.items)) {
		return payload.items;
	}
	return [];
}

/**
 * The scope a read was bounded by, as the engine declared it: `all`,
 * `except_global` or `restricted`.
 *
 * The engine answers every read with it, so the agent can tell "there are
 * no memories" from "you may not read them". An older engine does not
 * declare it, and then the answer says nothing.
 */
export function readScope(payload) {
	if (payload && typeof payload === "object" && typeof payload.read_scope === "string") {
		return payload.read_scope;
	}
	return "";
}

/**
 * One line telling the agent how bounded the answer was. Empty when the
 * engine did not say (an older engine, or a payload without the field).
 */
export function scopeLine(payload) {
	const scope = readScope(payload);
	if (!scope) {
		return "";
	}
	if (scope === "all") {
		return "Read scope: all (every namespace is readable).";
	}
	if (scope === "except_global") {
		return "Read scope: except_global (the reserved namespace is not readable).";
	}
	return "Read scope: restricted (this profile may not read every namespace).";
}

/**
 * The namespace the engine says it searched.
 *
 * The one-call door declares it, so an empty answer can be told apart
 * from an answer that looked in another project. An older engine does not
 * declare it, and then the answer says nothing.
 */
export function searchedNamespace(payload) {
	if (payload && typeof payload === "object" && typeof payload.searched_namespace === "string") {
		return payload.searched_namespace === "" ? null : payload.searched_namespace;
	}
	return null;
}

/**
 * What the answer did not include: the candidates the budget could not
 * keep, with the size that explains why.
 *
 * The engine declares it (door-priced-answer S2): the agent's cross-check
 * round is a coverage problem, not a trust problem, and it needs something
 * to look for. An older engine does not declare it, and then the answer
 * says nothing.
 */
export function readCoverage(payload) {
	const coverage = payload && typeof payload === "object" ? payload.coverage : null;
	if (!coverage || typeof coverage !== "object" || Array.isArray(coverage)) {
		return null;
	}
	const items = Array.isArray(coverage.left_out_items) ? coverage.left_out_items : [];
	const leftOut = typeof coverage.left_out === "number" ? coverage.left_out : items.length;
	if (leftOut === 0 && items.length === 0) {
		return null;
	}
	return {
		candidates: typeof coverage.candidates === "number" ? coverage.candidates : 0,
		carried: typeof coverage.carried === "number" ? coverage.carried : 0,
		left_out: leftOut,
		left_out_items: items,
	};
}

/**
 * The declaration in the answer text: what the answer did not include.
 *
 * One line says how many candidates the budget could not keep, and one line
 * per candidate names it with the size that explains why it did not fit —
 * the record is one `mnexis_show` away.
 */
export function coverageLines(coverage) {
	if (!coverage || typeof coverage !== "object") {
		return [];
	}
	const items = Array.isArray(coverage.left_out_items) ? coverage.left_out_items : [];
	const leftOut = typeof coverage.left_out === "number" ? coverage.left_out : items.length;
	if (leftOut === 0 && items.length === 0) {
		return [];
	}
	const plural = (count) => `${count} candidate${count === 1 ? "" : "s"}`;
	const lines = [
		`Not included: ${plural(leftOut)} did not fit the budget (the closest to the cut first).`,
	];
	for (const item of items) {
		const kind = item?.memory_type ?? "fact";
		const title = item?.title ?? "";
		const uuid = item?.uuid ?? "";
		const bytes = typeof item?.bytes === "number" ? item.bytes : 0;
		lines.push(`- [${kind}] ${title} — ${uuid} (${bytes} bytes)`);
	}
	return lines;
}

/**
 * The relations between the memories a payload holds: which one replaced
 * which.
 *
 * The answer says it in the same breath as the memories, so the agent does
 * not have to open every record to know which one is current.
 */
export function readRelations(payload) {
	if (payload && typeof payload === "object" && Array.isArray(payload.relations)) {
		return payload.relations;
	}
	return [];
}

/**
 * One line per relation, in the words the agent needs: "this one was
 * replaced by that one".
 */
export function relationLines(relations) {
	if (!Array.isArray(relations) || relations.length === 0) {
		return "";
	}
	return (
		"\nRelations: " +
		relations
			.map((r) => {
				const reason = r && r.reason ? ` (${r.reason})` : "";
				return `${r.older} was replaced by ${r.newer}${reason}`;
			})
			.join("; ")
	);
}

/**
 * The session-state block a door's answer carries, when it says something.
 *
 * The block is additive and optional: an engine that predates it sends no
 * `session_state`, and a namespace with nothing to say sends an empty one.
 * Both answer exactly as the door always did — a block with nothing in it is
 * not worth the lines the agent pays for, and `null` is not a fact.
 *
 * @param {unknown} payload
 * @returns {{ records: unknown[], index: unknown[], session: object | null } | null}
 */
export function readSessionState(payload) {
	const block = payload && typeof payload === "object" ? payload.session_state : null;
	if (!block || typeof block !== "object" || Array.isArray(block)) {
		return null;
	}
	const records = Array.isArray(block.records) ? block.records : [];
	const index = Array.isArray(block.index) ? block.index : [];
	const session =
		block.session && typeof block.session === "object" && !Array.isArray(block.session)
			? block.session
			: null;
	if (records.length === 0 && index.length === 0 && session === null) {
		return null;
	}
	return { ...block, records, index, session };
}

/**
 * The block in the answer text, in the items' own style: a JSON object per
 * line.
 *
 * The two heading lines are what makes the block readable at a glance: the
 * first says how many records ride resident and what they cost, the second
 * says that some live state is *indexed* — it exists, it did not fit, and
 * `mnexis_show` pulls it. What the budget left out is never silent even
 * though its content is not here (state-block-budget S2, S5, S7.2).
 */
export function stateBlockLines(block) {
	if (!block || typeof block !== "object") {
		return [];
	}
	const records = Array.isArray(block.records) ? block.records : [];
	const index = Array.isArray(block.index) ? block.index : [];
	const session = block.session && typeof block.session === "object" ? block.session : null;
	const plural = (count) => `${count} record${count === 1 ? "" : "s"}`;

	const budget = typeof block.budget === "number" ? block.budget : null;
	const resident = typeof block.resident_tokens === "number" ? block.resident_tokens : null;
	const lines = [
		budget === null || resident === null
			? `session state: ${plural(records.length)}`
			: `session state: ${plural(records.length)}, ${resident} of ${budget} tokens`,
	];
	lines.push(...records.map((record) => JSON.stringify(record)));
	if (index.length > 0) {
		lines.push(
			`indexed: ${plural(index.length)} did not fit the budget ` +
				"(pull one with mnexis show <uuid>)",
		);
		lines.push(...index.map((entry) => JSON.stringify(entry)));
	}
	if (session) {
		lines.push(`session: ${JSON.stringify(session)}`);
	}
	return lines;
}

/**
 * The namespace a read tool targets.
 *
 * Reads are open across projects: without an explicit namespace the tool
 * does not narrow the search, so the agent sees what other projects
 * recorded. An explicit namespace narrows it to that one project.
 *
 * @param {unknown} value
 * @returns {string | undefined}
 */
export function readNamespace(value) {
	const namespace = typeof value === "string" ? value.trim() : "";
	return namespace === "" ? undefined : namespace;
}

/**
 * Check that every memory a write touches lives in the workspace
 * namespace.
 *
 * A write that crosses a project boundary is refused before it reaches
 * the CLI: this workspace writes only into its own namespace. A memory
 * that cannot be read cannot be checked either, and a write whose target
 * is unknown is refused too.
 *
 * @returns {Promise<{ message: string, details: Record<string, unknown> } | null>}
 */
export async function checkProjectBoundary(client, uuids, namespace, cwd) {
	for (const uuid of uuids) {
		const result = await client.show(uuid, cwd);
		// A read the engine did not answer is not a memory in another
		// namespace: it is a read that failed, and nothing was changed (S3).
		if (result.code !== 0 || result.no_answer) {
			return {
				message:
					`Cannot read the memory ${uuid} to check which project it belongs to, ` +
					`so nothing was changed. This workspace writes only into '${namespace}'.`,
				details: { uuid, namespace },
			};
		}
		const owner =
			result.data && typeof result.data === "object" ? result.data.namespace : undefined;
		if (owner !== namespace) {
			return {
				message:
					`The memory ${uuid} lives in namespace '${owner ?? "unknown"}' and this ` +
					`workspace writes only into '${namespace}'. Saving a memory for another project ` +
					`is the human's decision, not the agent's.`,
				details: { uuid, namespace: owner ?? null, workspace: namespace },
			};
		}
	}
	return null;
}

/** Build the tool result shape pi expects. */
function ok(text, details) {
	return { content: [{ type: "text", text }], details };
}

/** Build a failed tool result that still carries structured data. */
function fail(text, details) {
	return { content: [{ type: "text", text }], details, isError: true };
}

// --- the loadout (odd/tasks/ext-ts-rewrite.md, rules 1-3) ---
//
// The model is declared five tools and reaches the other nine through
// `tool_search`: `deferred` and `hidden` tools are registered but never
// declared, and the two lifecycle tools are declared inactive because the
// extension runs the lifecycle itself. What makes that safe is the pair of
// helpers below — `tool_search` is activated for the deferred tail, and an
// answer that implies a tail tool activates it.

/** The built-in tool that loads a `deferred` tool (AB1). */
const TOOL_SEARCH = "tool_search";

/**
 * The tail tools a successful answer implies (AB2).
 *
 * The door hands back uuids and the count of what it left out; a search
 * answer says to read the hit. Both are useless unless the tool that reads
 * them can be called, and the tail is deferred — so the call that implied it
 * activates it: one delta on the active set, no `tool_search` round trip. A
 * save's answer implies nothing, because the record is already written.
 */
const IMPLIED_TAIL_TOOLS = {
	mnexis_context: ["mnexis_show", "mnexis_history"],
	mnexis_search: ["mnexis_show"],
	mnexis_save: [],
};

/**
 * The names the host declares to the model right now, or null when it cannot
 * say.
 */
function activeToolNames(pi) {
	if (typeof pi?.getActiveTools !== "function") {
		return null;
	}
	try {
		const names = pi.getActiveTools();
		if (!Array.isArray(names)) {
			return null;
		}
		return names.filter((name) => typeof name === "string");
	} catch {
		return null;
	}
}

/**
 * Add tools to the host's active set (AB1, AB2).
 *
 * Additive and idempotent: the set pi holds is read and only the missing names
 * are appended, so an active tool is never declared twice and nothing the
 * human activated is dropped. A host that cannot report or change the set is
 * left alone silently — the loadout is an improvement, never a precondition.
 */
function activateTools(pi, names) {
	if (typeof pi?.setActiveTools !== "function") {
		return false;
	}
	const wanted = Array.isArray(names)
		? names.filter((name) => typeof name === "string" && name !== "")
		: [];
	const current = activeToolNames(pi);
	if (current === null) {
		return false;
	}
	const added = wanted.filter((name) => !current.includes(name));
	if (added.length === 0) {
		return false;
	}
	try {
		pi.setActiveTools([...current, ...added]);
		return true;
	} catch {
		return false;
	}
}

/**
 * Activate the built-in `tool_search` when the host offers one that reaches
 * the deferred tail (AB1).
 *
 * pi registers `tool_search` inactive and activates it for its own MCP tools;
 * nothing does that for another extension's deferred tools, and without it the
 * tail is seven dead ends that fail before any handler runs. The guard is
 * pi's own: a tool of that name that declares no `reachesTools` belongs to
 * someone else, and is never activated.
 */
function activateToolSearch(pi) {
	if (typeof pi?.getAllTools !== "function") {
		return false;
	}
	let tools;
	try {
		tools = pi.getAllTools();
	} catch {
		return false;
	}
	if (!Array.isArray(tools)) {
		return false;
	}
	const search = tools.find(
		(tool) => tool && tool.name === TOOL_SEARCH && Boolean(tool.reachesTools),
	);
	if (!search) {
		return false;
	}
	return activateTools(pi, [TOOL_SEARCH]);
}

/**
 * Commands and file writes that would grant write permissions without the
 * human. The extension owns that decision, so a model-issued call that
 * tries to do it behind the extension's back is blocked.
 */
const PERMISSION_GRANT_PATTERN =
	/\bmnexis\s+config\s+allow-namespace\b|\bmnexis\s+project\s+bind\b/;

/** True when a bash command would grant namespace permissions on its own. */
export function isPermissionGrantCommand(command) {
	if (typeof command !== "string") {
		return false;
	}
	return PERMISSION_GRANT_PATTERN.test(command);
}

/** True when a file write would edit the profile by hand. */
export function isProfileFilePath(path) {
	if (typeof path !== "string") {
		return false;
	}
	return /(?:^|[\\/])(mnexis|projects)\.toml$/.test(path);
}

/**
 * True when the profile settings file is missing and `mnexis init` must run
 * before anything else. `mnexis config show --json` answers with
 * `config_file_present`.
 */
export function needsProfileInit(configData) {
	if (!configData || typeof configData !== "object") {
		return true;
	}
	return configData.config_file_present === false;
}

/**
 * The id of the session the extension runs in, when the host has one.
 *
 * The id belongs to the host: the agent never invents one, and the
 * extension only passes it along.
 */
export function sessionIdOf(ctx) {
	if (!ctx || !ctx.sessionManager) {
		return null;
	}
	try {
		const id = ctx.sessionManager.getSessionId();
		return typeof id === "string" && id !== "" ? id : null;
	} catch {
		return null;
	}
}

/**
 * The canonical title of the state a closing session leaves behind
 * (capture-discipline S2).
 *
 * The title is the engine's identity: `key_from_title` makes the same title
 * the same record, so every close lands on the record the previous close
 * left and bumps its revision instead of adding a second answer to "where
 * did the last session leave the project". One workspace keeps exactly one
 * live `project_state` under this title, however many sessions close.
 */
export const SESSION_CLOSE_TITLE = "estado al cierre";

/**
 * The mechanical state a closed session leaves in the engine.
 *
 * Code can only state what the engine answered: the session id, its
 * boundaries, the directory it ran in and the commit it started from. The
 * summary is the narrative, and the engine's session already carries it (the
 * door's session-state block shows it), so the mechanical record does not
 * duplicate it and its budget does not pay for it twice. Nothing is invented,
 * and a fact the engine did not answer is omitted instead of written as
 * `undefined`.
 */
export function sessionCloseContent(session) {
	const data = session && typeof session === "object" ? session : {};
	const text = (value) => (typeof value === "string" ? value.trim() : "");
	const id = text(data.id) || "unknown";
	const startedAt = text(data.started_at);
	const endedAt = text(data.ended_at);
	const directory = text(data.directory);
	const branch = text(data.branch);
	const branchSha = text(data.branch_sha);
	const branchRemote = text(data.branch_remote);

	const lines = [`Session ${id} ended${endedAt ? ` at ${endedAt}` : ""}.`];
	if (startedAt) {
		lines.push(`It started at ${startedAt}.`);
	}
	if (directory) {
		lines.push(`Directory: ${directory}.`);
	}
	if (branch) {
		const at = branchSha ? ` at ${branchSha}` : "";
		const remote = branchRemote ? ` (${branchRemote})` : "";
		lines.push(`Branch: ${branch}${at}${remote}.`);
	}
	return lines.join("\n");
}

/**
 * The question the settle nudge asks (capture-discipline S4, AB5).
 *
 * The hooks capture the mechanical floor; whether a session deserved a
 * memory is judgment. This is the text the model reads when the session's
 * own events show unsaved work, and it is spent once per session: it asks
 * for the judgment, not for a quota.
 *
 * It is one judgment line and a pointer: what deserves to survive, and the
 * seven record types one line each, are stated by `mnexis_save`'s own
 * description, where the model reads them as it writes (AB5). The budget is
 * per fire (S5) — the nudge no longer pays for doctrine the tools block
 * already carries.
 */
export const SETTLE_NUDGE = [
	"This session changed files but saved no memory. If a decision, preference, discovery or project-state change deserves to survive the session, save it now with one mnexis_save; otherwise reply that nothing deserves memory.",
	"",
	"`mnexis_save` states what deserves to survive and the seven record types, one line each, so the save arrives typed.",
].join("\n");

/**
 * The longest gate refusal the nudge carries, in characters.
 *
 * The gate's messages are shorter than this; the bound exists so the nudge's
 * per-fire budget cannot be spent on an answer the model could have read.
 */
const REFUSAL_CHARS = 400;

/**
 * The question, with the gate's own reason when a save was denied (AB6).
 *
 * A denied save is the one failure whose reason the model cannot read from
 * the engine: the gate is the extension's decision, and the command a human
 * must run is the only way the session's memory gets written.
 */
function settleNudgeText(refusal) {
	const reason = typeof refusal === "string" ? refusal.trim() : "";
	if (reason === "") {
		return SETTLE_NUDGE;
	}
	const carried = reason.length > REFUSAL_CHARS ? `${reason.slice(0, REFUSAL_CHARS - 1)}…` : reason;
	return `${SETTLE_NUDGE}\n\nThe last mnexis_save was refused: ${carried}`;
}

/**
 * The reason a failed `mnexis_save` gave, when the write gate is what refused
 * it (AB6).
 *
 * The gate is the extension's own decision, so `authorized: false` is how its
 * refusal is told apart from an engine failure the model already read in the
 * answer. The text is the message the model got: the command a human must run,
 * or the namespace that was declined.
 */
function gateRefusalOf(event) {
	const details = event?.result?.details;
	if (!details || typeof details !== "object" || details.authorized !== false) {
		return "";
	}
	const content = Array.isArray(event?.result?.content) ? event.result.content : [];
	for (const part of content) {
		if (part && part.type === "text" && typeof part.text === "string") {
			return part.text.trim();
		}
	}
	return "";
}

/** The tools whose successful run changed a file (capture-discipline S4). */
const FILE_WRITING_TOOLS = new Set(["write", "edit"]);

/**
 * How long the workspace's own state may take to answer.
 *
 * The snapshot is a nicety, not the evidence the session is judged on: a
 * repository whose status takes longer than this is a repository the
 * question is asked without, never a session that hangs.
 */
const WORKSPACE_SNAPSHOT_TIMEOUT_MS = 2_000;

// --- store recovery (odd/tasks/store-recovery.md, S1-S7) ---
//
// A save that lands on a second live record for one fact leaves the store
// with two current answers to the same question. The extension repairs that
// one shape, and only that shape: it supersedes **every** older record that
// is a twin of this save onto the one the save wrote, so the fact keeps
// exactly one live record and the superseded rows stay visible as
// `superseded` (S1, S3, S5).

/**
 * The size of every page the repair reads to find a twin.
 *
 * It is the engine's own cap (`MAX_LIMIT = 100`, `src/cli/commands.rs:24`):
 * asking for more is silently clamped, and a bound nobody can reason about is
 * a bound that hides records. What matters is not the number but what happens
 * when it is reached — the engine answers with the records that carry the
 * value, and a page shorter than the bound is the end of them.
 */
export const TWIN_SCAN_LIMIT = 100;

/**
 * How many pages of **matches** the repair reads before it stops.
 *
 * The repair no longer walks the namespace: the engine filters by value, so
 * the pages it reads are the records that carry the value and nothing else.
 * The bound is on those matches (20 pages is 2000 records that carry the
 * same number, far past any project this engine serves), and past it the
 * repair reports that it could not check rather than reading forever (S4).
 */
export const TWIN_SCAN_MAX_PAGES = 20;

/**
 * The numeric values that identify a fact: its tokens of three or more digits.
 *
 * This is the bench's signature (R1), kept as measured: the failure corpus
 * is two records that carry the same number under two titles.
 */
export function numericValues(text) {
	const values = new Set();
	for (const match of String(text ?? "").match(/\d{3,}/g) ?? []) {
		values.add(match);
	}
	return values;
}

/**
 * The value two contents share when they carry the SAME numeric signature, or
 * null when they do not.
 *
 * A shared token is not enough: a save that says "the port is 9090, it used to
 * be 8081" shares 8081 with the record it contradicts, and repairing that would
 * make a value CHANGE look like a rename. Only an identical set is one fact
 * renamed. The cost is stated where it belongs: a rename that introduces an
 * unrelated number is not repaired, because a missed repair is a store that
 * looks like today's and a wrong one is a fact that silently changed identity.
 */
export function sameNumericSignature(left, right) {
	const leftValues = numericValues(left);
	const rightValues = numericValues(right);
	if (leftValues.size === 0 || leftValues.size !== rightValues.size) {
		return null;
	}
	for (const value of leftValues) {
		if (!rightValues.has(value)) {
			return null;
		}
	}
	for (const value of leftValues) {
		return value;
	}
	return null;
}

/**
 * One keyword compared the way a reader compares it: case and spacing folded.
 *
 * A keyword is an element of the record's vocabulary, never a value, so
 * `listen port` and `port` are two different keywords on purpose: comparing
 * their words would widen the match, and this rule is allowed only to narrow.
 */
export function keywordKey(word) {
	return String(word ?? "")
		.trim()
		.toLowerCase()
		.replace(/\s+/g, " ");
}

/**
 * The keyword list of a record, however the engine or the save spelled it.
 */
function keywordList(words) {
	if (Array.isArray(words)) {
		return words;
	}
	if (typeof words === "string") {
		return words.split(",");
	}
	return [];
}

/**
 * True when two records share at least one keyword (S2.3).
 *
 * This is the hardening the product adds to the measured rule: a shared
 * number alone also matches unrelated facts, such as a port and a worker
 * count (L2). No shared keyword means no repair, and the product accepts
 * missing a twin rather than retiring an unrelated record.
 */
export function sharesKeyword(left, right) {
	const rightKeys = new Set(keywordList(right).map(keywordKey));
	for (const word of keywordList(left)) {
		const key = keywordKey(word);
		if (key !== "" && rightKeys.has(key)) {
			return true;
		}
	}
	return false;
}

/**
 * The canonical title of a session's compaction record (R4).
 *
 * The title is the engine's identity: `key_from_title` makes the same title
 * the same record, so an untitled save wrote one row per compaction and never
 * revised one. The session is what the record is about, so the session names
 * it, and a second compaction of the same session revises the first one.
 */
export function compactionTitle(sessionId) {
	return `session ${sessionId ?? "unknown"} compaction`;
}

/**
 * The topic key the same record is filed under: the session's, explicitly.
 *
 * The title already derives one, and the pair says the same thing twice on
 * purpose: the key is what a caller reads to prove the identity is the
 * session, and it stays inside the engine's 128-byte cap for any session id.
 */
export function compactionTopicKey(sessionId) {
	return `session.${sessionId ?? "unknown"}.compaction`;
}

/**
 * The compaction summary the shipped `session_compact` hook stores is not a
 * project fact (S2.4).
 *
 * The hook writes `Session <id> was compacted. What was happening: ...`; that
 * text is session history, and two records that quote it are not one fact.
 */
export function isCompactionSummary(item) {
	return /\bwas compacted\b/i.test(String(item?.content ?? ""));
}

/**
 * True when a record carries a vocabulary this rule can compare.
 *
 * A `mnexis search` hit carries the words; a record read whole always does.
 * An answer without them is not judged as "no shared keyword" — it is read
 * whole and judged by `twinValue`, so a missing field can never turn the
 * repair into a silent no-op.
 */
function hasKeywordField(item) {
	return Array.isArray(item?.keywords) || typeof item?.keywords === "string";
}

/**
 * True when the two records can be the same kind of fact, judged on the type
 * each one declares.
 *
 * The type is part of what a fact IS: a `knowledge` record and a `decision`
 * record that quote the same value and share a word are two facts the team
 * keeps separately, and superseding one onto the other loses the kind of one
 * of them (A4). A type the answer does not carry cannot be compared, and this
 * rule never refuses on a field it cannot read: the record is read whole
 * before a pair is formed.
 */
function sameKnownType(left, right) {
	const a = typeof left?.memory_type === "string" ? left.memory_type.trim() : "";
	const b = typeof right?.memory_type === "string" ? right.memory_type.trim() : "";
	return a === "" || b === "" || a === b;
}

/**
 * True when `item` could be the live twin of the record a save just wrote,
 * judged on the identity a ranked hit already answers (S2.1, S2.3).
 *
 * The cheap half of the rule, deliberately: a search hit carries the uuid,
 * the state, the namespace, the words and the type, so everything except the
 * numeric signature and the summary test is decided before a record is read
 * whole. A superseded row is history, not a second live record; a record in
 * another namespace is another project's; a record of another type is another
 * kind of fact (A4); no shared keyword is another fact that happens to carry
 * the same number (L2).
 */
export function sharesIdentity(saved, item, namespace) {
	if (!saved || typeof saved !== "object" || !item || typeof item !== "object") {
		return false;
	}
	if (typeof namespace !== "string" || namespace === "") {
		return false;
	}
	if (typeof saved.uuid !== "string" || saved.uuid === "" || saved.namespace !== namespace) {
		return false;
	}
	if (typeof item.uuid !== "string" || item.uuid === "" || item.uuid === saved.uuid) {
		return false;
	}
	if ((item.state ?? "active") !== "active") {
		return false;
	}
	if (item.namespace !== namespace) {
		return false;
	}
	// The kind of thing the record is: another type is another fact, never a
	// second live record of this one (A4).
	if (!sameKnownType(saved, item)) {
		return false;
	}
	return !hasKeywordField(item) || sharesKeyword(saved.keywords, item.keywords);
}

/**
 * The shared value when `item` is a live twin of `saved`, or null (S2).
 *
 * The expensive half: it needs both contents, so it runs on records that
 * already passed `sharesIdentity` and, for a ranked hit, after reading the
 * record whole. The title is not compared — the measured failure had two
 * different titles, and the engine already updates on a title match.
 */
export function twinValue(saved, item, namespace) {
	if (!sharesIdentity(saved, item, namespace)) {
		return null;
	}
	// Now the words are authoritative: the identity half lets an answer
	// without them through only so it can be read, never so it can be paired.
	if (!sharesKeyword(saved.keywords, item.keywords)) {
		return null;
	}
	if (isCompactionSummary(saved) || isCompactionSummary(item)) {
		return null;
	}
	return sameNumericSignature(saved.content, item.content);
}

/**
 * How many live records of the namespace carry a value, other than the
 * record the save just wrote (S1).
 *
 * It is read from the answer the repair already has: `list --value <N>`
 * answers every record that carries the value in the save's namespace, with
 * its state and its uuid, so counting them costs no second engine call. A
 * superseded row is history, not a second live fact, and does not carry a
 * value in the sense this rule means. The record the save wrote is excluded
 * because the pair under evaluation is always the twins against it.
 *
 * The uuids are deduplicated: the pages are disjoint by offset, and a count
 * that trusted that would double on a retry.
 */
function liveCarriers(items, namespace, savedUuid) {
	const uuids = new Set();
	for (const item of Array.isArray(items) ? items : []) {
		if (!item || typeof item !== "object") {
			continue;
		}
		if (item.namespace !== namespace) {
			continue;
		}
		if ((item.state ?? "active") !== "active") {
			continue;
		}
		if (typeof item.uuid !== "string" || item.uuid === "" || item.uuid === savedUuid) {
			continue;
		}
		uuids.add(item.uuid);
	}
	return uuids.size;
}

/**
 * The values of the save's signature that are ambient, and so are not
 * identity evidence (S1).
 *
 * A value identifies a fact only while it is RARE: when the live records of
 * the namespace that carry it are the records of this fact and no others
 * (the human's decision, odd/tasks/trust-batch.md, L2). A year, a repeated
 * ADR id and every other incidental number are carried by unrelated live
 * records, and two facts that share one are two facts, however identical
 * their numeric signatures look.
 *
 * `carried` is, per value of the save, how many live records of the
 * namespace carry it other than the record the save wrote; `twins` is how
 * many of those records this fact is. A value is ambient when MORE live
 * records carry it than that. The threshold is what keeps the rule from
 * swallowing real duplicates: two or three records of one fact pair among
 * themselves, because they are all the carriers there are, and a value one
 * more live record also carries never pairs. A caller that did not count
 * passes no map and no value is judged ambient — the behaviour before this
 * rule.
 */
function ambientValues(saved, carried, twins) {
	const ambient = [];
	if (!(carried instanceof Map)) {
		return ambient;
	}
	for (const value of numericValues(saved?.content)) {
		const carriers = carried.get(value);
		if (typeof carriers === "number" && carriers > twins) {
			ambient.push(value);
		}
	}
	return ambient;
}

/**
 * Every live twin the candidate query answered, as pairs (S1, B3).
 *
 * All of them: S1 promises the fact ends with exactly one live record, and
 * retiring one of N twins delivers "one fewer", not "one". The order is the
 * engine's own id order, which is the order a reader expects the older
 * records in.
 *
 * The evidence is the whole signature, so one ambient value refuses every
 * pair: the values of a twin are the values of the save, and a signature that
 * includes a number the namespace carries in other live records is not
 * identity evidence.
 */
export function twinPairs(saved, candidates, namespace, carried = null) {
	const pairs = [];
	const seen = new Set();
	for (const item of Array.isArray(candidates) ? candidates : []) {
		if (!item || typeof item !== "object") {
			continue;
		}
		if (typeof item.uuid === "string" && seen.has(item.uuid)) {
			continue;
		}
		const value = twinValue(saved, item, namespace);
		if (value === null) {
			continue;
		}
		seen.add(item.uuid);
		pairs.push({ older: item, newer: saved, value });
	}
	if (pairs.length === 0) {
		return pairs;
	}
	const ambient = ambientValues(saved, carried, pairs.length);
	return ambient.length === 0 ? pairs : [];
}

/** The name of a record in an answer a human or the model reads. */
function recordLabel(item) {
	const uuid = typeof item?.uuid === "string" && item.uuid !== "" ? item.uuid : "unknown";
	const title = typeof item?.title === "string" ? item.title.trim() : "";
	return title === "" ? uuid : `'${title}' (${uuid})`;
}

/** Why the older record was superseded: the link reason the engine keeps. */
export function twinReason(pair) {
	return (
		`one fact, one record: ${recordLabel(pair.older)} and ${recordLabel(pair.newer)} ` +
		`carried the same value ${pair.value}`
	);
}

/**
 * The one line the model reads per repaired twin (S4).
 *
 * It names both records and the shared value, and says what the store did
 * with the older one: superseded, never deleted, still visible as
 * `superseded` in `mnexis list` and `mnexis show`. The engine also keeps the
 * state the flip replaced in `memory_history`
 * (`src/storage/memory_repo.rs`, `supersede_atomic`), so `mnexis_history`
 * and `mnexis show --revision <old revision>` answer the record as it was
 * before the supersede; this one line does not narrate the trail, because
 * the trail is a store fact the model can ask for and not a second sentence
 * on every repair.
 */
export function twinRepairLine(pair) {
	return (
		`Repaired a duplicate: ${recordLabel(pair.older)} and ${recordLabel(pair.newer)} ` +
		`carried the same value ${pair.value} for one fact; the older record was ` +
		"superseded, not deleted, and `mnexis list` and `mnexis show` report it as `superseded`."
	);
}

/**
 * The article the type of a fact reads with: `an episode`, `a fact`.
 *
 * Only the first sound matters for the closed set of types the engine accepts
 * (`src/domain/memory_type.rs`), and the answer is read by a human.
 */
function typeArticle(type) {
	return /^[aeiou]/i.test(String(type ?? "")) ? "an" : "a";
}

export default function mnexisMemoryExtension(pi: ExtensionAPI) {
	// One client, one child registry and one resolution cache per extension
	// instance (per session). The registry holds the children the hooks start,
	// so a shutdown can end what it left behind (R8).
	const children = createChildRegistry();
	const client = createClient({
		exec: defaultExec,
		binary: process.env.MNEXIS_BIN,
		env: process.env,
		registry: children,
	});
	let resolution = null;
	let writeNamespaces = null;
	// S4: the session's own evidence, cheap enough to keep per event. The
	// settle nudges only when the session changed files and saved nothing,
	// and only once per session; a fresh session starts with nothing here.
	let sessionFilesChanged = false;
	// The evidence is the outcome, not the attempt (AB6): a save counts only
	// once the engine answered it, so a save the gate denied or the engine
	// failed leaves the question open. `sessionSaveRefusal` is why the gate
	// said no, and the nudge is where the model reads it again.
	let sessionSaved = false;
	let sessionSaveRefusal = "";
	let settleNudgeSent = false;
	// The workspace's own state when the session started: the paths git sees
	// as changed or untracked. A session that changes a file through `bash`
	// — a `chmod`, a `sed -i`, a redirect — shows the extension no
	// write/edit tool, and the pilot of the checklist bench measured 6 of 41
	// sessions invisible to the tool names alone. The workspace itself is the
	// evidence, and this is the state to compare it against. Null when the
	// workspace is not a repository: no evidence is not a negative answer.
	let sessionWorkspaceBaseline = null;
	// UX2: the session's adoption counters, on the footer key the tools
	// already write. Set at the session's start, so a session that reads
	// nothing still says so.
	const footer = createFooter();
	// The messages the `input` hook captured in this session: a fact about
	// the session, not about the store.
	let sessionPromptsCaptured = 0;
	// What the engine answered when the session was registered (R3): the
	// `/mnexis` card reports it from here instead of asking again.
	let sessionRegistration = { state: "unknown", ended_at: null, namespace: null };
	// AB3: the cached truth of the conditional prompt section. Null means
	// "add nothing": unbound, empty, or a store the probe could not read.
	let memorySection = null;

	/** Register one memory tool with this session's footer (UX2). */
	const registerTool = (definition) => registerMemoryTool(pi, definition, footer);

	// The journal's renderers (UX1) and the nudge's (UX4): registered once, at
	// load, and guarded for a host that predates them.
	registerJournalRenderers(pi);
	registerNudgeRenderer(pi);

	/**
	 * The client a hook call uses (R8).
	 *
	 * Hook calls carry no signal from pi, so they carry the session's: a
	 * shutdown kills what a hook started, and the await that was still in
	 * flight ends as cancelled instead of hanging on a dead session. The
	 * registry is read at call time, because a shutdown spends its signal and
	 * the next session of the same runtime gets a live one.
	 */
	const hookClient = () => client.withSignal(children.signal);

	/**
	 * What a branch owns: the evidence, the spent question and the state the
	 * workspace was in when the branch began (R6).
	 *
	 * A session starts on a branch, and `/tree` moves it to another one of the
	 * same session. The latch of a question spent on the branch we left must
	 * not silence the one the new branch owes, and the writes of the old branch
	 * are not the work of the new one — so the trio and the baseline are reset
	 * together, by both boundaries.
	 */
	async function resetEvidence(ctx) {
		sessionFilesChanged = false;
		sessionSaved = false;
		sessionSaveRefusal = "";
		settleNudgeSent = false;
		// What the workspace looked like when this branch began: the state to
		// compare the settle boundary against, so a change the tools did not
		// report (a file written through `bash`) is still the branch's work.
		sessionWorkspaceBaseline = await workspaceState(hookClient(), ctx?.cwd);
	}

	/**
	 * The workspace's own state, as git sees it: the commit it is at and the
	 * paths that are changed or untracked.
	 *
	 * Two questions, because they fail differently: the commit moves when
	 * the session committed its work (the tree is clean again, and the
	 * status alone would say nothing happened), and the paths move when the
	 * session changed a file without committing it.
	 *
	 * Null when the workspace is not a repository, or git is not there, or the
	 * call did not answer in time: no evidence is not a negative answer, and
	 * the tool's own evidence stands on its own.
	 *
	 * @param {object} client the engine client
	 * @param {string} cwd the workspace to look at
	 * @returns {Promise<string|null>} the state, or null when there is none
	 */
	async function workspaceState(client, cwd) {
		if (typeof cwd !== "string" || cwd === "") {
			return null;
		}
		try {
			const head = await client.git(["rev-parse", "HEAD"], cwd, WORKSPACE_SNAPSHOT_TIMEOUT_MS);
			const status = await client.git(
				["status", "--porcelain"],
				cwd,
				WORKSPACE_SNAPSHOT_TIMEOUT_MS,
			);
			const at = head?.code === 0 ? head.value : null;
			const paths = status?.code === 0 ? status.value : null;
			if (at === null && paths === null) {
				return null;
			}
			return `${at ?? ""}\n${paths ?? ""}`;
		} catch {
			return null;
		}
	}

	/**
	 * Resolve the workspace → namespace binding once per session.
	 *
	 * The binding is a fact the engine owns, so a `project list` it did not
	 * answer is not "this workspace has no namespace": the resolution carries
	 * the failure and nothing is cached, because a workspace the human bound is
	 * never reported as unbound and a hung engine is not asked twice in one call
	 * (S2, S3).
	 */
	async function resolveWorkspace(cli, cwd) {
		if (resolution) {
			return resolution;
		}
		const root = await cli.git(["rev-parse", "--show-toplevel"], cwd);
		const repoRoot = root.code === 0 && root.value ? root.value : null;
		const remote = await cli.git(["config", "--get", "remote.origin.url"], repoRoot || cwd);
		const remoteUrl = remote.code === 0 && remote.value ? remote.value : null;
		const key = workspaceKey({ remote: remoteUrl, repoRoot, cwd });
		const rootPath = repoRoot || cwd;

		const list = await cli.projectList(cwd);
		const failure =
			list.code !== 0
				? describeFailure({ ...list })
				: list.no_answer
					? engineSilence(list)
					: null;
		if (failure) {
			return {
				key,
				namespace: null,
				bound: false,
				candidate: null,
				refusal: null,
				root: rootPath,
				remote: remoteUrl,
				failure,
			};
		}

		const bindings = list.data && Array.isArray(list.data.bindings) ? list.data.bindings : [];
		// The engine owns the identity rule: the registry publishes the
		// canonical key of the workspace this process runs in, so the
		// binding the human recorded and the one the agent looks up are
		// the same key. The computed key stays as the fallback for an
		// older engine that does not publish it yet.
		const publishedKey =
			list.data && typeof list.data.current_key === "string" && list.data.current_key !== ""
				? list.data.current_key
				: null;
		const canonicalKey = publishedKey ?? key;
		// A `.mnexis-namespace` marker names the namespace of the
		// workspace; the mnexis CLI resolves it the same way, so both
		// sides agree on which project a directory belongs to. The marker
		// is a file inside the workspace, so it can only NAME a candidate:
		// nothing is bound from here (auto-namespace S6).
		const marked = findMarkerNamespace(rootPath);
		const selection = selectNamespace({ bindings, key: canonicalKey, fallbackName: marked });

		// The candidate namespace a human would be offered (auto-namespace
		// S2). The engine derives it, so the extension reads the engine's
		// answer instead of deriving a second copy that could drift.
		let candidate = { namespace: null, refusal: null };
		if (!selection.bound) {
			if (marked) {
				// The name a human wrote down wins over the name a remote
				// yields — and is still only a candidate.
				candidate = candidateFromMarker(marked);
			} else {
				const status = await cli.status(cwd);
				const published = readWorkspaceCandidate(status.code === 0 ? status.data : null);
				// Only an engine that cannot answer leaves the derivation to
				// the extension: the rules in lib/workspace.js (the last
				// segment of the remote, lowercased, or the repository
				// directory's name) are the engine's own rules, kept for an
				// older or unreachable engine.
				candidate = published.answered
					? published
					: deriveWorkspaceCandidate({ remote: remoteUrl, repoRoot, cwd });
			}
		}

		resolution = {
			key: canonicalKey,
			namespace: selection.namespace,
			bound: selection.bound,
			candidate: candidate.namespace,
			refusal: candidate.refusal,
			root: rootPath,
			remote: remoteUrl,
		};
		return resolution;
	}

	/**
	 * The paragraph the conditional prompt section carries (AB3).
	 *
	 * One short English paragraph, and three facts: which namespace holds this
	 * project's memories, roughly how many there are, and the two calls that
	 * read them. It is only true when the workspace is bound and the namespace
	 * is not empty, which is what the cached probe decides.
	 */
	function memorySectionText(section) {
		const held = section.capped
			? `at least ${section.count} memories`
			: section.count === 1
				? "one memory"
				: `${section.count} memories`;
		return (
			`The namespace '${section.namespace}' holds ${held} recorded about this project: the ` +
			"decisions and discoveries about this code live there. Call mnexis_context with a " +
			"one-line task summary at the start of a session, and mnexis_search before changing " +
			"code that already exists."
		);
	}

	/**
	 * Decide once, at the session's start, whether the prompt section is true
	 * (AB3).
	 *
	 * One bounded engine call — a cheap count of the workspace's own namespace —
	 * and the result is cached, so the handler that runs on every run touches no
	 * engine at all. A probe that cannot answer is journaled and adds nothing:
	 * the section is an improvement, never a precondition, and the prompt is
	 * never told about memories nobody could read.
	 */
	async function cacheMemorySection(ctx) {
		memorySection = null;
		const deadline = createDeadline(
			budgetOf(process.env.MNEXIS_MEMORY_PROBE_BUDGET_MS, MEMORY_PROBE_BUDGET_MS),
			"memory probe",
		);
		try {
			const workspace = await resolveWorkspace(hookClient(), ctx?.cwd);
			// Only a bound workspace names a namespace that is a permission, and
			// only a namespace with memories gives the section something to say.
			if (workspace.failure || !workspace.bound || !workspace.namespace) {
				return;
			}
			const namespace = workspace.namespace;
			const step = await deadline.step("list", (left) =>
				hookClient()
					.withBudget(left)
					.list({ namespace, limit: TWIN_SCAN_LIMIT }, ctx?.cwd),
			);
			if (step.abandoned) {
				appendRecovery(ctx, {
					action: "failed",
					step: "memory-probe",
					namespace,
					error: step.abandoned,
				});
				return;
			}
			const answer = step.value;
			if (answer.code !== 0) {
				appendRecovery(ctx, {
					action: "failed",
					step: "memory-probe",
					namespace,
					exit_code: answer.code,
					error: describeFailure({ ...answer, namespace }),
				});
				return;
			}
			if (answer.no_answer) {
				appendRecovery(ctx, {
					action: "failed",
					step: "memory-probe",
					namespace,
					error: engineSilence(answer),
				});
				return;
			}
			const count = readItems(answer.data).length;
			if (count === 0) {
				return;
			}
			memorySection = { namespace, count, capped: count >= TWIN_SCAN_LIMIT };
		} catch (error) {
			appendRecovery(ctx, {
				action: "failed",
				step: "memory-probe",
				error: error instanceof Error ? error.message : String(error),
			});
		}
	}

	/**
	 * The `/mnexis` card (UX3): what the session may read, where it writes, and
	 * how it has used the engine so far.
	 *
	 * Everything but the memory count is the extension's own cached state — the
	 * workspace's resolution, the engine's answer to the session's start, the
	 * captured messages, the footer's counters and the nudge's latch. The count
	 * is one bounded engine call with the prompt capture's short budget: a
	 * card that cannot be made inside it says why instead of blocking the
	 * human's command.
	 */
	async function statusCard(ctx) {
		const deadline = createDeadline(
			budgetOf(process.env.MNEXIS_STATUS_BUDGET_MS, STATUS_CARD_BUDGET_MS),
			"status card",
		);
		let answer = null;
		let engine = null;
		try {
			const step = await deadline.step("status", (left) =>
				hookClient().withBudget(left).status(ctx?.cwd),
			);
			if (step.abandoned) {
				engine = { error: step.abandoned };
			} else if (step.value.code !== 0) {
				engine = { error: describeFailure({ ...step.value }) };
			} else if (step.value.no_answer) {
				engine = { error: engineSilence(step.value) };
			} else {
				answer =
					step.value.data && typeof step.value.data === "object" ? step.value.data : null;
			}
		} catch (error) {
			engine = { error: error instanceof Error ? error.message : String(error) };
		}
		const workspace =
			answer?.workspace && typeof answer.workspace === "object" ? answer.workspace : {};
		const scope = answer?.scope && typeof answer.scope === "object" ? answer.scope : {};
		const namespace =
			typeof workspace.namespace === "string" && workspace.namespace !== ""
				? workspace.namespace
				: null;
		const counts = footer.counts();
		return {
			ts: new Date().toISOString(),
			kind: "status",
			workspace:
				typeof workspace.key === "string" && workspace.key !== "" ? workspace.key : null,
			namespace,
			bound: namespace !== null,
			read_namespaces: Array.isArray(scope.read_namespaces) ? scope.read_namespaces : [],
			write_namespaces: Array.isArray(scope.write_namespaces) ? scope.write_namespaces : [],
			memories: typeof answer?.memory_count === "number" ? answer.memory_count : null,
			// The engine's count is what the profile's read scope sees: with reads
			// open it reaches past the workspace, and the card says so instead of
			// calling it the namespace's own count.
			memories_read_all: answer?.scope?.allow_read_all === true,
			session: sessionIdOf(ctx),
			session_state: sessionRegistration.state,
			session_ended_at: sessionRegistration.ended_at ?? null,
			prompts_captured: sessionPromptsCaptured,
			counters: counts,
			footer: counterLine(counts),
			nudge: settleNudgeSent ? "sent" : "open",
			...(engine ? { engine } : {}),
		};
	}

	/** The one line the `/mnexis` command says after writing its card. */
	function statusSummaryLine(card) {
		const namespace = card.namespace ?? "unbound";
		const memories =
			typeof card.memories === "number" ? `${card.memories} memories` : "memories unknown";
		return (
			`mnexis · ${namespace} · ${memories} · ` +
			`ctx ${card.counters.ctx} · search ${card.counters.search} · save ${card.counters.save}`
		);
	}

	/**
	 * Register `/mnexis` (UX3).
	 *
	 * Read-only by design: the card reports the binding and the allowlists, and
	 * the only subcommand there is, is `status`. Binding or granting a
	 * namespace touches the permission surface, and that decision is the
	 * human's — the design deferred any grant path to its own decision
	 * (ext-ts-rewrite, UX3).
	 */
	function registerStatusCommand() {
		if (typeof pi?.registerCommand !== "function") {
			return;
		}
		try {
			pi.registerCommand("mnexis", {
				description:
					"Show this session's mnexis memory state as a durable card (read-only): " +
					"/mnexis status",
				getArgumentCompletions(prefix) {
					const typed = String(prefix ?? "").trim().toLowerCase();
					const options = [
						{
							value: "status",
							label: "status",
							description:
								"the workspace, the session and how this session used the engine",
						},
					];
					const matching = options.filter((option) => option.value.startsWith(typed));
					return matching.length === 0 ? null : matching;
				},
				async handler(args, ctx) {
					const subcommand = String(args ?? "").trim().toLowerCase();
					if (subcommand !== "" && subcommand !== "status") {
						if (ctx?.ui && typeof ctx.ui.notify === "function") {
							ctx.ui.notify(
								`mnexis: '${subcommand}' is not a subcommand. Only 'status' exists: ` +
									"/mnexis status shows the memory state of this session.",
								"info",
							);
						}
						return;
					}
					const card = await statusCard(ctx);
					if (typeof pi.appendEntry === "function") {
						try {
							pi.appendEntry("mnexis-status", card);
						} catch {
							// The card is a report; the line below still says what it holds.
						}
					}
					if (ctx?.ui && typeof ctx.ui.notify === "function") {
						ctx.ui.notify(statusSummaryLine(card), "info");
					}
				},
			});
		} catch {
			// A host that refuses the command still has the tools.
		}
	}

	registerStatusCommand();

	/**
	 * Read the write allowlist from the engine, initializing the profile if
	 * needed.
	 *
	 * It answers a failure instead of throwing: a write tool that throws cannot
	 * tell the model what happened, and a profile read the engine never answered
	 * is not an empty allowlist (S3, audit A8).
	 */
	async function readProfileAllowlist(cli, cwd) {
		let config = await cli.configShow(cwd);
		if (config.code !== 0) {
			return { message: describeFailure({ ...config }) };
		}
		if (config.no_answer) {
			return { message: engineSilence(config) };
		}
		// `mnexis project bind` works without a profile, so the binding can
		// exist while the profile does not. Every write needs the profile.
		if (needsProfileInit(config.data)) {
			const init = await cli.init(cwd);
			if (init.code !== 0) {
				return { message: describeFailure({ ...init }) };
			}
			config = await cli.configShow(cwd);
			if (config.code !== 0) {
				return { message: describeFailure({ ...config }) };
			}
			if (config.no_answer) {
				return { message: engineSilence(config) };
			}
		}
		return { namespaces: readWriteNamespaces(config.data) };
	}

	/**
	 * The session's write allowlist, read once and re-read on demand.
	 *
	 * The cache is a cost saver, never a verdict about the human's profile:
	 * `fresh` reads the profile again, because a grant the human made in
	 * another terminal is what the retry promised by the refusal must find
	 * (A1).
	 */
	async function getWriteNamespaces(cli, cwd, { fresh = false } = {}) {
		if (!fresh && writeNamespaces) {
			return { namespaces: writeNamespaces };
		}
		const read = await readProfileAllowlist(cli, cwd);
		if (read.message) {
			return read;
		}
		writeNamespaces = read.namespaces;
		return read;
	}

	/**
	 * Ensure the namespace may be written to, asking the human once.
	 *
	 * Interactive sessions confirm through pi's UI; headless sessions never
	 * authorize anything and report the exact command a human must run.
	 */
	async function ensureWritable(cli, namespace, ctx) {
		const unreadable = (read) => ({
			authorized: false,
			asked: false,
			message: `${read.message} No memory was written.`,
		});
		let read = await getWriteNamespaces(cli, ctx.cwd);
		if (read.message) {
			return unreadable(read);
		}
		let allowed = read.namespaces;
		if (!allowed.includes(namespace)) {
			// The refusal below promises that running the command makes the
			// retry work, so a cached allowlist is never the last word: the
			// profile is read again before the write is refused (A1). An
			// authorized write still pays for one read per session.
			read = await getWriteNamespaces(cli, ctx.cwd, { fresh: true });
			if (read.message) {
				return unreadable(read);
			}
			allowed = read.namespaces;
		}
		const decision = decideAuthorization({
			namespace,
			writeNamespaces: allowed,
			hasUI: Boolean(ctx.hasUI),
		});
		if (decision.action === "allow") {
			return { authorized: true, asked: false };
		}
		if (decision.action === "pending") {
			return {
				authorized: false,
				asked: false,
				message: authorizationPendingMessage(namespace, decision.reason),
			};
		}
		// R5: the dialog carries a timeout, and its three answers are told
		// apart — a session without a UI, a no, and nobody answering are three
		// different things and the model reads which one it met.
		//
		// pi's signature is confirm(title, message): the short label is the
		// title and the whole question is the message. They were swapped here
		// (L9's queued defect), so the dialog showed a five-line paragraph as
		// its title.
		const answer = await askConfirm(
			ctx.ui,
			"mnexis: authorize namespace",
			`Allow the mnexis agent to write memories in namespace '${namespace}'?\n\n` +
				"This grants write permission (save, update, supersede) for that namespace only, " +
				"in the current mnexis profile. Other namespaces and global access stay untouched, " +
				"and you can revoke it later by editing the profile allowlist.",
			dialogTimeoutMs(),
		);
		if (answer === "timed_out") {
			return {
				authorized: false,
				asked: true,
				message: authorizationPendingMessage(namespace, DIALOG_TIMEOUT_REASON),
			};
		}
		if (answer !== "confirmed") {
			return {
				authorized: false,
				asked: true,
				message: `Authorization for namespace '${namespace}' was declined. No memory was written.`,
			};
		}
		const allow = await cli.allowNamespace(namespace, ctx.cwd);
		if (allow.code !== 0) {
			return {
				authorized: false,
				asked: true,
				message: describeFailure({ ...allow, namespace }),
			};
		}
		writeNamespaces = [...allowed, namespace];
		return { authorized: true, asked: true };
	}

	/**
	 * The question a human answers once per workspace (auto-namespace S3).
	 *
	 * It names the four things the decision needs — the namespace, the
	 * workspace key, the path, and what the grant allows — because a binding
	 * widens the read scope and the allowlist entry adds the write
	 * permission: both are permissions, so both are said out loud.
	 */
	function bindingPrompt(workspace) {
		return (
			`Bind this workspace to the namespace '${workspace.candidate}'?\n\n` +
			`Namespace: ${workspace.candidate}\n` +
			`Workspace key: ${workspace.key}\n` +
			`Path: ${workspace.root}\n\n` +
			`Saying yes grants two things at once: the binding widens the read scope to ` +
			`'${workspace.candidate}', and the allowlist entry lets the agent write memories ` +
			`there (save, update, supersede). Other namespaces and the reserved 'global' ` +
			`stay untouched, and you can revoke both later in the mnexis profile.`
		);
	}

	/** Why the write stops when the engine offers no candidate. */
	function noCandidateMessage(workspace) {
		return (
			`This workspace (${workspace.key}) has no namespace yet, and the agent ` +
			`does not choose one. Ask the human to run:\n` +
			`  mnexis project bind <namespace> --key ${workspace.key}\n` +
			`  mnexis config allow-namespace <namespace>\n` +
			`and retry. Reading is still available.`
		);
	}

	/** Why the write stops when the engine refused its own candidate. */
	function refusedCandidateMessage(workspace) {
		return (
			`This workspace (${workspace.key}) offers no candidate namespace, so the agent ` +
			`does not ask for one. Reason: ${workspace.refusal}\nNo memory was written.`
		);
	}

	/** The two commands a human must run when the session cannot ask. */
	function pendingBindingMessage(workspace, reason) {
		return (
			`This workspace (${workspace.key}) is not bound to a namespace. The engine offers ` +
			`'${workspace.candidate}' for it, and only a human can grant it: ${reason}. Ask the human to run:\n` +
			`  ${bindCommand(workspace.candidate, workspace.key, workspace.root)}\n` +
			`  ${allowNamespaceCommand(workspace.candidate)}\n` +
			`and retry. No memory was written.`
		);
	}

	/**
	 * Offer an unbound workspace's candidate namespace to the human, and
	 * grant it only on an explicit yes (auto-namespace S2, S3, S4, S6).
	 *
	 * One question per workspace, naming the namespace, the key, the path
	 * and the two permissions it grants. Nothing is bound or allowed without
	 * that answer: a headless session reports the exact commands instead of
	 * guessing, and a candidate the engine refused is reported with its
	 * reason instead of being asked about. The reserved `global` can never
	 * reach this path — the engine refuses it, and so does the marker rule.
	 */
	async function bindOnConfirmation(cli, workspace, ctx) {
		const decision = decideBinding({
			candidate: workspace.candidate,
			refusal: workspace.refusal,
			hasUI: Boolean(ctx.hasUI),
		});
		if (decision.action === "refuse") {
			return {
				workspace,
				gate: { authorized: false, asked: false, message: refusedCandidateMessage(workspace) },
			};
		}
		if (decision.action === "none") {
			return {
				workspace,
				gate: { authorized: false, asked: false, message: noCandidateMessage(workspace) },
			};
		}
		if (decision.action === "pending") {
			return {
				workspace,
				gate: {
					authorized: false,
					asked: false,
					message: pendingBindingMessage(workspace, decision.reason),
				},
			};
		}
		// The same order as the gate's authorization dialog: the short label is
		// pi's confirm title, the whole question is its message.
		const answer = await askConfirm(
			ctx.ui,
			"mnexis: name this workspace",
			bindingPrompt(workspace),
			dialogTimeoutMs(),
		);
		if (answer === "timed_out") {
			// The session had a UI and nobody answered it: the answer is the
			// pending one, with the reason the question went unanswered (R5).
			return {
				workspace,
				gate: {
					authorized: false,
					asked: true,
					message: pendingBindingMessage(workspace, DIALOG_TIMEOUT_REASON),
				},
			};
		}
		if (answer !== "confirmed") {
			return {
				workspace,
				gate: {
					authorized: false,
					asked: true,
					message:
						`Binding this workspace (${workspace.key}) to '${workspace.candidate}' was declined. ` +
						`No memory was written: nothing was bound and nothing was allowed.`,
				},
			};
		}
		const read = await getWriteNamespaces(cli, ctx.cwd);
		if (read.message) {
			return {
				workspace,
				gate: {
					authorized: false,
					asked: true,
					message:
						`${read.message} No memory was written: nothing was bound and ` +
						"nothing was allowed.",
				},
			};
		}
		const allowed = read.namespaces;
		const bind = await cli.bind(workspace.candidate, workspace.key, workspace.root, ctx.cwd);
		if (bind.code !== 0) {
			return {
				workspace,
				gate: {
					authorized: false,
					asked: true,
					message: `${describeFailure({
						...bind,
						namespace: workspace.candidate,
					})}. No memory was written.`,
				},
			};
		}
		const allow = await cli.allowNamespace(workspace.candidate, ctx.cwd);
		if (allow.code !== 0) {
			return {
				workspace,
				gate: {
					authorized: false,
					asked: true,
					message:
						`The workspace was bound to '${workspace.candidate}', but the write allowlist ` +
						`could not be updated: ${describeFailure({
							...allow,
							namespace: workspace.candidate,
						})}. No memory was written.`,
				},
			};
		}
		// The grant is real now: remember both halves — the binding the
		// resolution carries and the allowlist entry — so the rest of the
		// session writes without asking again.
		writeNamespaces = [...allowed, workspace.candidate];
		resolution = {
			...workspace,
			namespace: workspace.candidate,
			bound: true,
			candidate: null,
			refusal: null,
		};
		return { workspace: resolution, gate: { authorized: true, asked: true } };
	}

	/**
	 * Journal one line of store recovery (S4).
	 *
	 * The host session's entry journal is the path this extension's answers
	 * already live on: pi persists it beside the tool results and keeps it out
	 * of the model context, and the extension has no file of its own to log to.
	 * A journal that cannot be written must never break a save.
	 */
	function appendRecovery(ctx, data) {
		if (typeof pi.appendEntry !== "function") {
			return;
		}
		try {
			pi.appendEntry("mnexis-store-recovery", {
				ts: new Date().toISOString(),
				kind: "store-recovery",
				session: sessionIdOf(ctx),
				...data,
			});
		} catch {
			// The journal is a report, never a precondition.
		}
	}

	/**
	 * Journal a captured message the store refused.
	 *
	 * The store never truncates what a person said: a message over the content
	 * cap is refused whole. A refusal that only lived in a swallowed exit code
	 * would be invisible, so its size and its code reach the host's entry
	 * journal — and the text never does.
	 */
	function journalPromptSkip(ctx, data) {
		if (typeof pi.appendEntry !== "function") {
			return;
		}
		try {
			pi.appendEntry("mnexis-prompt-capture", {
				ts: new Date().toISOString(),
				kind: "prompt-capture",
				action: "skipped",
				session: sessionIdOf(ctx),
				...data,
			});
		} catch {
			// The journal is a report, never a precondition.
		}
	}

	/**
	 * Journal a session close the engine did not take (S2).
	 *
	 * A close is the one write no later session can reconstruct: the session
	 * is gone and its state was never written. A refusal that only lived in a
	 * swallowed exit code would leave the workspace silently without its
	 * estado al cierre, so the step, the exit code and the engine's own
	 * message reach the host's entry journal — never a narrative the store
	 * could not take.
	 */
	function journalSessionClose(ctx, data) {
		if (typeof pi.appendEntry !== "function") {
			return;
		}
		try {
			pi.appendEntry("mnexis-session-close", {
				ts: new Date().toISOString(),
				kind: "session-close",
				action: "skipped",
				session: sessionIdOf(ctx),
				...data,
			});
		} catch {
			// The journal is a report, never a precondition.
		}
	}

	/**
	 * Journal a compaction summary the store did not take.
	 *
	 * A compaction that cannot be stored never stops the session, and the
	 * summary cannot be replayed either: the context it described is gone. A
	 * refusal that only lived in a swallowed exit code would be invisible, so
	 * the code and the engine's own message reach the host's entry journal —
	 * and never the summary's text, which stays the human's conversation.
	 *
	 * The entry type stays one of the four the extension declares: `action`
	 * says whether the store refused the summary (`skipped`) or the host could
	 * not compact at all (`failed`, R4).
	 */
	function journalCompactionSkip(ctx, data) {
		if (typeof pi.appendEntry !== "function") {
			return;
		}
		try {
			pi.appendEntry("mnexis-compaction", {
				ts: new Date().toISOString(),
				kind: "compaction-capture",
				action: "skipped",
				session: sessionIdOf(ctx),
				...data,
			});
		} catch {
			// The journal is a report, never a precondition.
		}
	}

	/**
	 * The records that carry a value, as the engine answers them (S2, B1).
	 *
	 * The engine has a filter by value (`mnexis list --value <N>`): it
	 * answers with the records whose indexed text carries the value, and the
	 * filter applies BEFORE the limit, so the answer is a page of matches
	 * and not a short answer. The engine orders a list by id and pages it by
	 * offset, so consecutive offsets cover every match once: a page shorter
	 * than the bound is the end, and nothing can sit behind it.
	 *
	 * This is the whole namespace the engine reads no more: the records
	 * that do not carry the value are never read, so the repair's cost
	 * follows the matches and not the size of the namespace.
	 */
	async function listByValue(cli, namespace, value, ctx) {
		const items = [];
		for (let page = 0; page < TWIN_SCAN_MAX_PAGES; page += 1) {
			const listed = await cli.list(
				{
					namespace,
					value,
					limit: TWIN_SCAN_LIMIT,
					offset: page * TWIN_SCAN_LIMIT,
					full: true,
				},
				ctx.cwd,
			);
			if (listed.code !== 0) {
				return {
					error: {
						step: "list",
						value,
						error: describeFailure({ ...listed, namespace }),
					},
				};
			}
			if (listed.no_answer) {
				return { error: { step: "list", value, error: engineSilence(listed) } };
			}
			const rows = readItems(listed.data);
			items.push(...rows);
			if (rows.length < TWIN_SCAN_LIMIT) {
				return { items };
			}
		}
		return {
			error: {
				step: "list",
				value,
				error:
					"more than " +
					`${TWIN_SCAN_LIMIT * TWIN_SCAN_MAX_PAGES} records carry the value ${value}, ` +
					"so the twin scan stopped before it could prove it had seen them all",
			},
		};
	}

	/**
	 * The records that could be a twin of this save (S2, B1, B2).
	 *
	 * The engine is asked for the records that carry the value, never for a
	 * page of the namespace: every record the engine answers is a record
	 * that carries a value of this save, and a twin is one of those.
	 *
	 * The answer is the **intersection** over the values of this save: a
	 * twin carries ALL of them, so a record that carries only one of them
	 * is not a twin and is never read whole. The engine's answer is a
	 * superset of the records whose content carries the value (it reads the
	 * title and the keywords too), so the intersection can only lose a
	 * record the twin rule would have rejected anyway.
	 */
	async function candidatesByValue(cli, saved, ctx) {
		let candidates = null;
		const carried = new Map();
		for (const value of numericValues(saved.content)) {
			const answered = await listByValue(cli, saved.namespace, value, ctx);
			if (answered.error) {
				return answered;
			}
			// S1: the same answer that names the records carrying the value
			// counts the live ones, so an ambient value is judged without a
			// second engine call.
			carried.set(value, liveCarriers(answered.items, saved.namespace, saved.uuid));
			if (candidates === null) {
				candidates = answered.items;
				continue;
			}
			const carriedByPage = new Set(answered.items.map((item) => item.uuid));
			candidates = candidates.filter((item) => carriedByPage.has(item.uuid));
		}
		return { items: candidates ?? [], source: "value", carried };
	}

	/**
	 * The whole record a candidate needs to be judged (S2.2, S2.4).
	 *
	 * A `list --full` page already carries the content; a ranked hit carries
	 * the identity only, so only the records that survived the cheap half of
	 * the rule are read whole — a twin is checked against what the store says,
	 * never against a preview.
	 */
	async function readCandidate(cli, item, ctx) {
		if (typeof item.content === "string") {
			return { record: item };
		}
		const shown = await cli.show(item.uuid, ctx.cwd);
		if (shown.code !== 0) {
			return {
				error: {
					step: "show",
					uuid: item.uuid,
					error: describeFailure({
						...shown,
						namespace: item.namespace,
					}),
				},
			};
		}
		if (shown.no_answer) {
			return { error: { step: "show", uuid: item.uuid, error: engineSilence(shown) } };
		}
		if (!shown.data || typeof shown.data !== "object" || typeof shown.data.uuid !== "string") {
			return { error: { step: "show", uuid: item.uuid, error: "the engine answered no record" } };
		}
		return { record: shown.data };
	}

	/**
	 * Repair every second live record a save just created (S1-S5, B1-B3).
	 *
	 * Placement is deliberate: this runs in `mnexis_save`'s own execute, after
	 * the engine answered and after `prepareWrite` authorized the write, so
	 * the record the save wrote is the one the engine named — no correlation
	 * by tool-call id, no second write path, and no repair on a save the gate
	 * refused.
	 *
	 * The saved record is not read back: its uuid, its content and its words
	 * are the ones this call just wrote, and the engine stores the content
	 * verbatim (`src/application/service.rs:247`). What this call cannot know
	 * is the rest of the namespace, and that is the only thing read here.
	 *
	 * A repair that cannot read or write the store leaves the save's answer
	 * untouched and journals why (S4): the store is never guessed at, and a
	 * repair that did not happen is never silent.
	 */
	async function repairSavedTwin(cli, workspace, params, answer, ctx) {
		const savedUuid = typeof answer?.uuid === "string" ? answer.uuid : "";
		const content = typeof params?.content === "string" ? params.content : "";
		// No value of three or more digits means no twin can exist (S2.2), and
		// then the store is not read at all.
		if (savedUuid === "" || numericValues(content).size === 0) {
			return null;
		}
		const namespace = typeof workspace?.namespace === "string" ? workspace.namespace : "";
		// The `session_compact` guard, exactly (S3): only a bound workspace
		// names a namespace that is a permission, and only the allowlist lets
		// this session write into it. Neither is a failure to report: a save the
		// gate refused never reached here, so there is nothing to repair.
		if (workspace?.bound !== true || namespace === "") {
			return null;
		}
		const saved = {
			uuid: savedUuid,
			namespace,
			title:
				typeof answer?.title === "string" && answer.title.trim() !== ""
					? answer.title
					: typeof params?.title === "string"
						? params.title
						: "",
			content,
			keywords: Array.isArray(answer?.keywords)
				? answer.keywords
				: keywordList(params?.keywords),
			// The type the record ended up with: the engine names it, and a save
			// that names none keeps the one the record already has. With neither,
			// the type is unknown here and the twin rule does not judge it (A4).
			memory_type:
				typeof answer?.memory_type === "string" && answer.memory_type.trim() !== ""
					? answer.memory_type
					: typeof params?.type === "string" && params.type.trim() !== ""
						? params.type
						: "",
		};
		try {
			const read = await getWriteNamespaces(cli, ctx.cwd);
			if (read.message) {
				const failure = { step: "profile", error: read.message };
				appendRecovery(ctx, {
					action: "failed",
					namespace,
					saved: { uuid: savedUuid },
					...failure,
				});
				return { failed: [failure] };
			}
			if (!read.namespaces.includes(namespace)) {
				return null;
			}
			const found = await candidatesByValue(cli, saved, ctx);
			if (found.error) {
				appendRecovery(ctx, {
					action: "failed",
					namespace,
					saved: { uuid: savedUuid },
					...found.error,
				});
				return { failed: [found.error] };
			}
			const candidates = [];
			const failed = [];
			for (const item of found.items) {
				if (!sharesIdentity(saved, item, namespace)) {
					continue;
				}
				const resolved = await readCandidate(cli, item, ctx);
				if (resolved.error) {
					appendRecovery(ctx, {
						action: "failed",
						namespace,
						saved: { uuid: savedUuid },
						...resolved.error,
					});
					failed.push(resolved.error);
					continue;
				}
				candidates.push(resolved.record);
			}
			const pairs = twinPairs(saved, candidates, namespace, found.carried);
			const repaired = [];
			for (const pair of pairs) {
				// S5 keeps the measured direction: the older record is
				// superseded onto the one the save wrote, which is the newer
				// record, and the engine keeps the row and the link.
				const superseded = await cli.supersede(
					{ older: pair.older.uuid, newer: savedUuid, reason: twinReason(pair) },
					ctx.cwd,
				);
				if (superseded.code !== 0) {
					const failure = {
						step: "supersede",
						uuid: pair.older.uuid,
						value: pair.value,
						error: describeFailure({
							...superseded,
							namespace,
						}),
					};
					appendRecovery(ctx, {
						action: "failed",
						namespace,
						saved: { uuid: savedUuid },
						...failure,
					});
					failed.push(failure);
					continue;
				}
				// S4, one line per intervention.
				appendRecovery(ctx, {
					action: "supersede",
					namespace,
					older: { uuid: pair.older.uuid, title: pair.older.title ?? "" },
					newer: { uuid: savedUuid, title: saved.title },
					value: pair.value,
					reason: twinReason(pair),
				});
				repaired.push(pair);
			}
			if (repaired.length === 0) {
				return failed.length > 0 ? { failed } : null;
			}
			return {
				line: repaired.map(twinRepairLine).join("\n"),
				detail: {
					action: "supersede",
					namespace,
					source: found.source,
					newer: { uuid: savedUuid, title: saved.title },
					superseded: repaired.map((pair) => ({
						uuid: pair.older.uuid,
						title: pair.older.title ?? "",
						value: pair.value,
					})),
					...(failed.length > 0 ? { failed } : {}),
				},
			};
		} catch (error) {
			// A repair that cannot read or write the store leaves the save's
			// answer exactly as the engine gave it, and says so in the journal.
			const failure = {
				step: "repair",
				error: error instanceof Error ? error.message : String(error),
			};
			appendRecovery(ctx, {
				action: "failed",
				namespace,
				saved: { uuid: savedUuid },
				...failure,
			});
			return { failed: [failure] };
		}
	}

	/** Shared prologue for the write tools. */
	async function prepareWrite(cli, ctx) {
		const workspace = await resolveWorkspace(cli, ctx.cwd);
		if (workspace.failure) {
			// The binding is a fact the engine owns: a call it never answered
			// is not "this workspace is unbound", so the write stops with the
			// failure instead of asking the human to bind what may already be
			// bound (S2, S3).
			return {
				workspace,
				gate: {
					authorized: false,
					asked: false,
					message: `${workspace.failure} No memory was written.`,
				},
			};
		}
		if (!workspace.bound) {
			// No binding: the namespace is the human's decision. The engine's
			// candidate is the question; it is never the answer on its own.
			return bindOnConfirmation(cli, workspace, ctx);
		}
		const gate = await ensureWritable(cli, workspace.namespace, ctx);
		return { workspace, gate };
	}

	// The session the extension runs in: the engine knows its boundaries
	// without the model having to remember them. Both hooks are best
	// effort — a session that was never registered is not an error, it is
	// a session the engine did not hear about.
	pi.on("session_start", async (_event, ctx) => {
		// A new session (new, resume, fork, reload) carries no evidence and
		// no spent question from the previous one. Reset before the engine
		// call so a session the engine does not know yet still starts clean.
		// A registry killed by the previous shutdown is spent, too: /new fires
		// the close and then the start in the same process (R8).
		children.rearm();
		// The deferred tail is only reachable while `tool_search` is active,
		// and pi registers it inactive (AB1). The loadout is declared here,
		// at the boundary the session starts on, and left alone on a host
		// that cannot change it.
		activateToolSearch(pi);
		// The evidence trio and the workspace baseline: the same reset a branch
		// switch makes (R6).
		await resetEvidence(ctx);
		// UX2: the adoption counters are the session's, so a new session starts
		// them at zero and shows the line before the first call — a session that
		// reads nothing still says so.
		footer.reset();
		footer.show(ctx?.ui);
		sessionPromptsCaptured = 0;
		sessionRegistration = { state: "unknown", ended_at: null, namespace: null };
		// AB3: the conditional prompt section is decided once, here, from one
		// bounded engine call, and never inside the handler that runs every run.
		await cacheMemorySection(ctx);
		const id = sessionIdOf(ctx);
		if (!id) {
			return;
		}
		// The engine is told about the session, and its answer is read (R3).
		// A start the engine refused — the profile is not initialised, the
		// namespace is not writable — used to be swallowed whole, and a
		// resumed id used to look started while the engine still held it as
		// closed.
		//
		// The engine's state machine, verified against the binary
		// (`test/parity.session-state.test.js`): a second start of the same id
		// is a no-op that answers the row it already has, and a start after an
		// end answers that same ended row — the engine never reopens a
		// session and never errors on either. So a taken start is not an
		// error, an ended id is not an error either, and neither is hidden.
		try {
			const started = await hookClient().sessionStart(
				{ sessionId: id, directory: ctx.cwd },
				ctx.cwd,
			);
			if (started.code !== 0) {
				sessionRegistration = {
					state: "failed",
					exit_code: started.code,
					ended_at: null,
					namespace: null,
				};
				journalSessionClose(ctx, {
					step: "start",
					exit_code: started.code,
					error: describeFailure({ ...started }),
				});
				return;
			}
			const session = started.data && typeof started.data === "object" ? started.data : null;
			const endedAt =
				session && typeof session.ended_at === "string" && session.ended_at !== ""
					? session.ended_at
					: null;
			// What the engine holds about this session, for the `/mnexis` card.
			sessionRegistration = {
				state: endedAt === null ? "started" : "ended",
				ended_at: endedAt,
				namespace:
					session && typeof session.namespace === "string" ? session.namespace : null,
			};
			if (endedAt !== null) {
				// The human is in a session the engine has already closed: a
				// resume of a quit-closed id, or a start on one. No engine call
				// reopens it, so the honest thing is to say so — the memories
				// still land, and the close still revises the estado al cierre.
				journalSessionClose(ctx, {
					action: "noted",
					step: "start",
					already_ended: true,
					ended_at: endedAt,
					reason:
						`the engine holds session '${id}' as ended (${endedAt}) and a start ` +
						"never reopens it: this session's memories are written and its close revises " +
						"the estado al cierre, but the engine's session window keeps the earlier end",
				});
			}
		} catch (error) {
			sessionRegistration = {
				state: "failed",
				ended_at: null,
				namespace: null,
			};
			journalSessionClose(ctx, {
				step: "start",
				error: error instanceof Error ? error.message : String(error),
			});
		}
	});

	// AB3: the conditional prompt section.
	//
	// One section, mutated in place into the host's own structured prompt
	// options, and only when the session's cached fact says it is true — the
	// workspace is bound and its namespace holds memories. The handler never
	// returns `systemPrompt` (and never calls the engine): pi diffs the sections
	// and appends what changed, while a replaced prompt busts the cache on every
	// run.
	pi.on("before_agent_start", (event) => {
		if (!memorySection) {
			return undefined;
		}
		const sections = event?.systemPromptOptions?.sections;
		if (!sections || typeof sections !== "object" || Array.isArray(sections)) {
			return undefined;
		}
		sections[MEMORY_SECTION_KEY] = memorySectionText(memorySection);
		return undefined;
	});

	// A branch switch inside the session (R6).
	//
	// `/tree` moves the session to another leaf: what was true of the branch
	// we left is not true of the one we are on. So the evidence trio, the spent
	// question and the workspace baseline are reset exactly as a new session
	// resets them — the journal entries stay, because they are the branch's own
	// data and pi keeps them with the leaf that wrote them.
	pi.on("session_tree", async (_event, ctx) => {
		await resetEvidence(ctx);
	});

	// The other end of that session: the engine learns it is over, and the
	// workspace keeps exactly one live "estado al cierre" `project_state` for
	// the next session to resume from (capture-discipline S2). The title is
	// the engine's identity, so a second close revises the record instead of
	// adding a second one. Code writes the mechanical part; the narrative
	// stays the session's (the engine already carries its summary).
	//
	// A reload replaces this extension's runtime, not the session: pi starts
	// the same session again right after, so closing it there would claim an
	// end that did not happen and leave the engine's copy of the live session
	// ended. A quit, a new session, a resume or a fork is an end, and is.
	//
	// Neither half may break the shutdown, and neither refusal may vanish:
	// the entry journal carries the step and the engine's code, never the
	// state the store could not take.
	pi.on("session_shutdown", async (event, ctx) => {
		// R8: the children this session started are the budget's first
		// casualty. Killing them here, before a single step of the close is
		// paid for, ends a capture or a compaction that was still in flight
		// and would otherwise finish a write into the session that is closing
		// — and it is idempotent, so a shutdown that runs twice is safe.
		children.killAll();
		if (event?.reason === "reload") {
			return;
		}
		const id = sessionIdOf(ctx);
		if (!id) {
			return;
		}
		// R1: pi awaits this hook with no timeout, so the close gets one of its
		// own and shares it between its steps. An engine that does not answer
		// inside what is left is abandoned and journaled; the next session may
		// retry, because the end is idempotent and the estado al cierre is one
		// record under one title.
		const deadline = createDeadline(
			budgetOf(process.env.MNEXIS_SHUTDOWN_BUDGET_MS, SHUTDOWN_BUDGET_MS),
			"shutdown",
		);
		const abandon = (step, reason, extra = {}) =>
			journalSessionClose(ctx, { action: "abandoned", step, reason, ...extra });
		try {
			const endStep = await deadline.step("end", (left) =>
				client.withBudget(left).sessionEnd({ sessionId: id }, ctx.cwd),
			);
			if (endStep.abandoned) {
				abandon("end", endStep.abandoned);
				return;
			}
			const ended = endStep.value;
			if (ended.code !== 0) {
				// Exit 3 is the engine saying it never heard of this session:
				// nothing to end, and nothing to write its state into. The
				// refusal is journaled with that name, because a close that
				// silently skipped one is indistinguishable from a lost close
				// (R3).
				journalSessionClose(ctx, {
					step: "end",
					exit_code: ended.code,
					...(ended.code === 3 ? { not_registered: true } : {}),
					error: describeFailure({ code: ended.code, stderr: ended.stderr, command: "end" }),
				});
				return;
			}
			const session = ended.data && typeof ended.data === "object" ? ended.data : null;
			const namespace =
				session && typeof session.namespace === "string" ? session.namespace : "";
			if (namespace === "") {
				// The engine owns the workspace's namespace; a close that would
				// have to guess it is not a close, and says so.
				journalSessionClose(ctx, {
					step: "end",
					error: "the engine ended the session but answered no namespace",
				});
				return;
			}
			const saved = await deadline.step("save", (left) =>
				client.withBudget(left).save(
					{
						namespace,
						title: SESSION_CLOSE_TITLE,
						type: "project_state",
						content: sessionCloseContent({ id, ...(session ?? {}) }),
					},
					ctx.cwd,
				),
			);
			if (saved.abandoned) {
				abandon("save", saved.abandoned, { namespace });
				return;
			}
			if (saved.value.code !== 0) {
				journalSessionClose(ctx, {
					step: "save",
					namespace,
					exit_code: saved.value.code,
					error: describeFailure({
						code: saved.value.code,
						stderr: saved.value.stderr,
						namespace,
						command: "save",
					}),
				});
			}
		} catch (error) {
			// The session is going away either way; a close that cannot reach
			// the engine is a report, never an exception in the shutdown path.
			journalSessionClose(ctx, {
				step: "hook",
				error: error instanceof Error ? error.message : String(error),
			});
		}
	});

	// The human's message, captured where it is said.
	//
	// `user_prompts` is the store's emptiest promise: the table, the session
	// link, the full-text index and the search all exist, and until now the
	// only writer was a tool the model had to be asked to call. The model is
	// not part of a fact this store exists to keep, so the capture is the
	// host's: the `input` hook sees the text the person typed, before pi
	// expands a skill or a prompt template, and the CLI stores it byte for
	// byte in the namespace this workspace's memories land in — the same
	// resolution the session takes — so the row joins the session it was said
	// in and the memories it produced.
	//
	// A capture that cannot be written never stops the turn: the message is
	// the human's, and a store that refuses it (a message over the content cap
	// is refused, never truncated) must not swallow the message silently
	// either — the size and the exit code go to the entry journal, the text
	// never does.
	//
	// It is also bounded (R7): the capture runs on every message, so it gets a
	// budget of its own instead of the write one, and a store that cannot take
	// the message inside it loses the message, not the turn. The abandonment
	// is journaled like every other refusal, and the turn proceeds.
	pi.on("input", async (event, ctx) => {
		const text = typeof event?.text === "string" ? event.text : "";
		// A message another extension sent is not something the human said.
		if (text === "" || event?.source === "extension") {
			return;
		}
		const bytes = Buffer.byteLength(text, "utf8");
		const capture = createDeadline(
			budgetOf(process.env.MNEXIS_PROMPT_CAPTURE_BUDGET_MS, PROMPT_CAPTURE_BUDGET_MS),
			"prompt capture",
		);
		let captured;
		try {
			captured = await capture.step("capture", (left) =>
				hookClient().withBudget(left).savePrompt(
					{ content: text, sessionId: sessionIdOf(ctx) },
					ctx.cwd,
				),
			);
		} catch (error) {
			journalPromptSkip(ctx, {
				bytes,
				error: error instanceof Error ? error.message : String(error),
			});
			return;
		}
		if (captured.abandoned) {
			journalPromptSkip(ctx, {
				action: "abandoned",
				bytes,
				reason: captured.abandoned,
			});
			return;
		}
		const result = captured.value;
		if (result.cancelled && children.signal.aborted) {
			// The session is closing and the registry killed this child on
			// purpose (R8): the turn it belonged to is over, and journaling
			// every quit would bury the losses that matter.
			return;
		}
		if (result.code !== 0) {
			journalPromptSkip(ctx, {
				bytes,
				exit_code: result.code,
			});
			return;
		}
		// The message landed: what `/mnexis` reports as captured this session.
		sessionPromptsCaptured += 1;
	});

	// --- S4: judgment, asked once and only when the evidence says so ---
	//
	// The hooks above capture the floor. Whether the session deserved a
	// memory is judgment, and a model that never considers the question is
	// the gap the dogfood measured. The extension does not put the question
	// into the tools block (S3); it asks the model once, at the final
	// actionable boundary, when the session's own events show unsaved work —
	// a successful write/edit and no save that landed (AB6). Read-only
	// sessions and sessions that saved pay nothing: no entry, no
	// continuation.
	//
	// The answer is the model's. `continue: true` is one more model request,
	// so the question is spent as soon as it is asked: a second settle in
	// the same session never opens it again.
	pi.on("tool_execution_end", (event, ctx) => {
		// A tool that did not run, or ran and failed, changed no file: it is
		// not evidence of unsaved work.
		if (event?.isError === true) {
			// A save that failed is not a save: the store was not written, so
			// the question stays open, and a gate refusal carries the reason
			// the model would otherwise never read (AB6).
			if (event?.toolName === "mnexis_save") {
				const refusal = gateRefusalOf(event);
				if (refusal !== "") {
					sessionSaveRefusal = refusal;
				}
			}
			return;
		}
		if (FILE_WRITING_TOOLS.has(event?.toolName)) {
			sessionFilesChanged = true;
		}
		// The evidence is the outcome (AB6). pi reports an aborted call as a
		// failed result, so `isError` is the whole test for "it landed".
		if (event?.toolName === "mnexis_save") {
			sessionSaved = true;
			// UX2: the footer counts the saves that landed, exactly as the
			// question does — a refused save is not a save.
			footer.count("save", ctx?.ui);
		}
	});

	pi.on("agent_before_settle", async (_event, ctx) => {
		// The workspace's own state is the other half of the evidence: a file
		// the model changed through `bash` is a changed file, and the tool
		// names alone do not say so. The check is made here, where the
		// question is asked, and only when the tools said nothing — the
		// common case pays no git call at all.
		if (!sessionFilesChanged && sessionWorkspaceBaseline !== null) {
			const current = await workspaceState(client, ctx?.cwd);
			if (current !== null && current !== sessionWorkspaceBaseline) {
				sessionFilesChanged = true;
			}
		}
		if (settleNudgeSent || !sessionFilesChanged || sessionSaved) {
			return undefined;
		}
		settleNudgeSent = true;
		return {
			entries: [
				{
					type: "custom_message",
					customType: "mnexis-settle-nudge",
					content: settleNudgeText(sessionSaveRefusal),
					// UX4: the human can see that the extension asked — it used to
					// be `display: false`, indistinguishable from agent spontaneity.
					display: true,
				},
			],
			continue: true,
		};
	});

	// Where a compaction lands. The summary of the compacted context is
	// what the session was doing; storing it means the next session can
	// recover it from the memory instead of from a context that no longer
	// exists. Only where the human already allowed writes.
	//
	// The summary is an episode of this session, not a project fact, so it is
	// stored as one. A store that refuses it (over the content cap, a dead
	// topic key) takes the summary with it either way; what it must never do
	// is take it silently, so a refusal reaches the entry journal (A5). A
	// workspace with no binding, or no write permission for it, never asked
	// the store to take anything: that is the auto-namespace boundary, not a
	// refusal, and it stays as quiet as it always was.
	pi.on("session_compact", async (event, ctx) => {
		const summary = event?.compactionEntry?.summary;
		if (typeof summary !== "string" || summary.trim() === "") {
			return;
		}
		let namespace = null;
		try {
			const workspace = await resolveWorkspace(hookClient(), ctx.cwd);
			// Only a bound workspace has a namespace that is a permission: a
			// marker names a candidate, and a file inside the workspace must
			// not become the target of a write on its own (auto-namespace S6).
			if (workspace.failure || !workspace.bound || !workspace.namespace) {
				return;
			}
			namespace = workspace.namespace;
			const read = await getWriteNamespaces(hookClient(), ctx.cwd);
			if (read.message || !read.namespaces.includes(namespace)) {
				return;
			}
			const id = sessionIdOf(ctx);
			const result = await client.save(
				{
					namespace,
					content:
						`Session ${id ?? "unknown"} was compacted. What was happening:\n\n${summary}`,
					// The record is "what this session was doing", so its identity is
					// the session (R4): one title and one topic key per session, which
					// revises the row the previous compaction left instead of adding
					// another one — and, naming its record, this save also retries a
					// locked store (R2).
					title: compactionTitle(id),
					topicKey: compactionTopicKey(id),
					// The type is the engine's to resolve and the caller's to name:
					// a summary is what happened in this session, so it is an
					// episode. The tag is the one comma string the engine reads.
					type: "episode",
					tags: "compaction",
				},
				ctx.cwd,
			);
			if (result.code !== 0) {
				journalCompactionSkip(ctx, {
					step: "save",
					namespace,
					exit_code: result.code,
					error: describeFailure({
						...result,
						namespace,
					}),
				});
			}
		} catch (error) {
			// A compaction must not fail because the memory could not take it,
			// and it must not vanish quietly either.
			journalCompactionSkip(ctx, {
				step: "hook",
				namespace,
				error: error instanceof Error ? error.message : String(error),
			});
		}
	});

	// The other half of the compaction: the host compacted and failed (R4).
	//
	// It used to have no handler at all, so a compaction that never happened —
	// an overflow the provider refused, a threshold compaction that errored —
	// left no trace anywhere the human could read. It goes to the same
	// `mnexis-compaction` entry as the `failed` variant: there is nothing to
	// store (the summary does not exist), so what is journaled is the reason
	// the host gave and whether it will retry the turn. An `aborted` compaction
	// is the human's own cancel and stays quiet: nothing was lost that they do
	// not already know, and the journal is for what would otherwise vanish.
	//
	// Branch summaries stay REJECTED (the design's decision, recorded again
	// here): a branch's narrative belongs to the branch, not to this session's
	// state, and no handler asks the host for one.
	pi.on("session_compact_failed", (event, ctx) => {
		if (event?.aborted === true) {
			return;
		}
		const message = typeof event?.errorMessage === "string" ? event.errorMessage.trim() : "";
		journalCompactionSkip(ctx, {
			action: "failed",
			step: "compact",
			reason: typeof event?.reason === "string" ? event.reason : "unknown",
			will_retry: event?.willRetry === true,
			from_extension: event?.fromExtension === true,
			error: message === "" ? "the host reported a failed compaction and no message" : message,
		});
	});

	// The gate lives in this extension, so a model-issued call must not
	// grant the permissions behind its back. The human's own commands are
	// untouched: only model tool calls are inspected here.
	pi.on("tool_call", async (event, ctx) => {
		// UX2: the adoption counters move on the call, in the column the tool
		// belongs to. A save is counted from its outcome instead (see
		// `tool_execution_end`), because an attempt is not a write.
		const column = FOOTER_COLUMNS.get(event?.toolName);
		if (column) {
			footer.count(column, ctx?.ui);
		}
		if (event.toolName === "bash" && isPermissionGrantCommand(event.input?.command)) {
			return {
				block: true,
				reason:
					"Granting a namespace is the human's decision. Use the mnexis tools " +
					"(they ask for confirmation once per namespace) or ask the human to run " +
					"`mnexis config allow-namespace <ns>` themselves.",
			};
		}
		if ((event.toolName === "write" || event.toolName === "edit") && isProfileFilePath(event.input?.path)) {
			return {
				block: true,
				reason:
					"The mnexis profile files are managed by the mnexis CLI, never edited by hand. " +
					"Use `mnexis init`, `mnexis project bind` or `mnexis config allow-namespace` instead.",
			};
		}
		return undefined;
	});

	registerTool({
		name: "mnexis_save",
		label: "Save a project memory",
		promptSnippet: "Store a durable memory of this project in the local mnexis engine.",
		description:
			"Store a durable memory of this project: content must stand alone months later; " +
			"the namespace is this workspace's. The title is what the fact IS and picks the " +
			"record — the same title always lands on the same record, so a changed fact " +
			"updates it. Keywords are other languages' or teams' names for it, never a value." +
			"\n\nWhat deserves to survive, for example: a decision an external change forced " +
			"(a version pin, a constraint that moved); a discovery about how this repo " +
			"actually works; a preference the user stated about how they want things done." +
			"\n\nSave it typed — pass the one that fits:\n" +
			"knowledge — a durable fact about the domain, the tools or the data.\n" +
			"decision — a choice made, and what it beat.\n" +
			"preference — how the user wants things done.\n" +
			"procedure — the steps that reliably achieve something.\n" +
			"project_state — where the work stands now.\n" +
			"episode — something that happened in a session.\n" +
			"fact — the neutral default when none of the others fit.",
		promptGuidelines: [
			"Before reporting the task done, record a durable decision, discovery or user " +
				"preference with mnexis_save.",
		],
		executionMode: "sequential",
		parameters: {
			type: "object",
			properties: {
				content: {
					type: "string",
					description: "One self-contained sentence (the engine admits 16 KiB by default).",
				},
				title: {
					type: "string",
					description: "Stable label of the fact (what it IS, not what it says now).",
				},
				tags: { type: "string", description: "Comma-separated tags." },
				keywords: {
					type: "string",
					description:
						"Comma-separated synonyms in other languages or jargon; vocabulary, " +
						"never a value. They accumulate across saves: the cap is 32 words for " +
						"the record, and the count is the union of the words it already carries " +
						"and the new ones.",
				},
				replace_keywords: {
					type: "boolean",
					description:
						"Replace the record's words with the ones you send instead of adding " +
						"to them; the way out when the list is near the 32-word cap.",
				},
				topic_key: { type: "string", description: "Stable key grouping facts about one topic." },
				type: {
					type: "string",
					description:
						"Optional: `knowledge`, `decision`, `preference`, `procedure`, " +
						"`project_state`, `episode` or `fact`. Omit and a new record is " +
						"stored as `fact`; a re-save keeps the type.",
				},
				idempotency_key: { type: "string", description: "Makes retries safe." },
			},
			required: ["content"],
			additionalProperties: false,
		},
		async execute(toolCallId, params, signal, onUpdate, ctx) {
			// pi's abort reaches the engine this call starts, and every call has a
			// budget even when nothing aborts it (S2).
			const cli = client.withSignal(signal);
			const { workspace, gate } = await prepareWrite(cli, ctx);
			if (!gate.authorized) {
				return fail(gate.message, { namespace: workspace.namespace, authorized: false });
			}
			const result = await cli.save(
				{
					namespace: workspace.namespace,
					content: params.content,
					title: params.title,
					tags: params.tags,
					keywords: params.keywords,
					replaceKeywords: params.replace_keywords,
					topicKey: params.topic_key,
					idempotencyKey: params.idempotency_key,
					type: params.type,
				},
				ctx.cwd,
			);
			if (result.code !== 0) {
				return fail(
					describeFailure({ ...result, namespace: workspace.namespace }),
					{ namespace: workspace.namespace },
				);
			}
			if (result.no_answer) {
				// A save that was not acknowledged is not a save, and it is not
				// an empty store either (S3).
				return fail(engineSilence(result, { wrote: true }), {
					namespace: workspace.namespace,
				});
			}
			// The answer implies no tail tool: the record is written (AB2).
			activateTools(pi, IMPLIED_TAIL_TOOLS.mnexis_save);
			// The same title lands on the same record, so a save can update
			// a fact instead of adding a second one. The model has to read
			// that it happened.
			const revision = result.data && result.data.revision;
			const topicKey = result.data && result.data.topic_key;
			// The type the record ended up with: the engine resolves it when
			// the caller names none, so the answer is the only place the model
			// can read what landed. When the answer does not name it, the type
			// this call asked for is the closest the extension can be to
			// honest; with neither, the answer says nothing about the type
			// rather than inventing one.
			const storedType = result.data && result.data.memory_type;
			const type =
				typeof storedType === "string" && storedType !== ""
					? storedType
					: typeof params.type === "string" && params.type !== ""
						? params.type
						: null;
			const asType = type ? ` as ${typeArticle(type)} ${type}` : "";
			const what =
				typeof revision === "number" && revision > 1
					? `Updated the memory in namespace '${workspace.namespace}' (revision ${revision}, topic key \`${topicKey}\`)${asType}.`
					: `Saved a memory in namespace '${workspace.namespace}'${asType}.`;
			// The words the record keeps after the save, not the ones this call
			// named: the vocabulary accumulates, so the answer is the only place
			// that knows what the memory carries now.
			const words =
				result.data && Array.isArray(result.data.keywords) ? result.data.keywords : [];
			const whatWithWords =
				words.length > 0 ? `${what} Words: ${words.join(", ")}.` : what;
			// S1-S5: every live record that is a twin of this one is retired
			// onto it before the model reads the answer, and the reply carries
			// both records and the shared value (S4). A repair that could not
			// run journals why and never changes the answer.
			const repair = await repairSavedTwin(cli, workspace, params, result.data, ctx);
			// A repair that could not run must never read as a clean save: the
			// ceiling of the scan is declared to the model, so a second live
			// record left behind is a fact the caller can see and check.
			const unfinished =
				repair && repair.failed && repair.failed.length > 0
					? `\nThe duplicate check did not finish: ${repair.failed[0].error}. ` +
						"A second live record for this fact may remain; confirm with " +
						"`mnexis_search` before trusting the store."
					: "";
			return ok(
				`${whatWithWords}${repair && repair.line ? `\n${repair.line}` : ""}${unfinished}`,
				{
					namespace: workspace.namespace,
					uuid: result.data && result.data.uuid ? result.data.uuid : null,
					title: (result.data && result.data.title) || params.title || null,
					keywords: words,
					topic_key: topicKey || null,
					revision: revision || null,
					memory_type: type,
					output: result.stdout.trim(),
					data: result.data,
					...(repair && repair.detail ? { recovery: repair.detail } : {}),
					...(repair && repair.failed && repair.failed.length > 0
						? { recovery_failed: repair.failed }
						: {}),
				},
			);
		},
	});

	registerTool({
		name: "mnexis_search",
		label: "Search memories",
		promptSnippet: "Recall what was recorded, across projects.",
		annotations: { readOnlyHint: true },
		promptGuidelines: [
			"Before changing code that already exists, call mnexis_search to find what is " +
				"already recorded about it, then mnexis_show the hit you need.",
		],
		description:
			"Search stored memories (full-text, ranked) before answering from scratch. " +
			"Reaches every project by default; `namespace` narrows it. Write the query as " +
			"you would say it: every word counts, a hyphen is part of a word (`staging-eu`).",
		parameters: {
			type: "object",
			properties: {
				query: { type: "string", description: "Search terms, in plain language." },
				limit: { type: "number", description: "Maximum number of hits (default: 10)." },
				namespace: {
					type: "string",
					description: "Optional namespace to narrow the search.",
				},
				match: {
					type: "string",
					enum: ["any", "all", "phrase"],
					description:
						"How the words are matched: any (default) counts every word, all requires " +
						"every word, phrase keeps the query as one exact phrase.",
				},
				type: {
					type: "string",
					description:
						"Optional: only these kinds (comma-separated, the seven in " +
						"`mnexis_save.type`).",
				},
			},
			required: ["query"],
			additionalProperties: false,
		},
		async execute(toolCallId, params, signal, onUpdate, ctx) {
			const cli = client.withSignal(signal);
			const namespace = readNamespace(params.namespace);
			const result = await cli.search(
				{ query: params.query, namespace, limit: params.limit, match: params.match, type: params.type },
				ctx.cwd,
			);
			if (result.code !== 0) {
				return fail(describeFailure({ ...result }), {
					namespace: namespace ?? null,
				});
			}
			if (result.no_answer) {
				// An engine that said nothing has not said "no memories" (S3).
				return fail(engineSilence(result), { namespace: namespace ?? null });
			}
			// The answer names the hit and says to read it: the read tool is
			// activated by the call that implied it (AB2).
			activateTools(pi, IMPLIED_TAIL_TOOLS.mnexis_search);
			const hits = readItems(result.data);
			const relations = readRelations(result.data);
			const scope = scopeLine(result.data);
			const text = hits.length
				? hits
						.map((hit, index) => `${index + 1}. ${JSON.stringify(hit)}`)
						.join("\n")
				: `No memories matched '${params.query ?? ""}'.`;
			return ok(text + relationLines(relations) + (scope ? `\n${scope}` : ""), {
				namespace: namespace ?? null,
				count: hits.length,
				hits,
				relations,
				read_scope: readScope(result.data),
			});
		},
	});

	registerTool({
		name: "mnexis_suggest_topic_key",
		label: "Suggest the topic key",
		promptSnippet: "Ask which topic key a fact is already recorded under.",
		exposure: "deferred",
		annotations: { readOnlyHint: true },
		description:
			"Ask which topic key a title yields and whether this project records it. Use " +
			"it before saving an unsure fact: the answer names the record the title would " +
			"update.",
		parameters: {
			type: "object",
			properties: {
				title: { type: "string", description: "The title of the fact you are about to record." },
				namespace: {
					type: "string",
					description: "Optional namespace to ask in; the workspace's one by default.",
				},
			},
			required: ["title"],
			additionalProperties: false,
		},
		async execute(toolCallId, params, signal, onUpdate, ctx) {
			// A read: it resolves the workspace namespace like the read-only
			// tools do — no write confirmation — so the answer is about the
			// project the agent is working in, not the profile's default
			// namespace (which the workspace guard refuses).
			const cli = client.withSignal(signal);
			const workspace = await resolveWorkspace(cli, ctx.cwd);
			if (workspace.failure) {
				return fail(workspace.failure, { namespace: null });
			}
			const namespace = readNamespace(params.namespace) ?? workspace.namespace;
			const result = await cli.suggestTopicKey({ title: params.title, namespace }, ctx.cwd);
			if (result.code !== 0) {
				return fail(describeFailure({ ...result, namespace }), {
					namespace: namespace ?? null,
				});
			}
			if (result.no_answer) {
				return fail(engineSilence(result), { namespace: namespace ?? null });
			}
			const data = result.data || {};
			const key = data.topic_key || "";
			const held = data.recorded_by;
			const text = !key
				? "This title yields no topic key."
				: held
					? `The key \`${key}\` is already recorded by ${held}: saving with this title updates that record.`
					: `The key \`${key}\` is free.`;
			return ok(text, {
				title: params.title,
				namespace: namespace ?? null,
				topic_key: key,
				recorded_by: held || null,
			});
		},
	});

	registerTool({
		name: "mnexis_list",
		label: "List memories",
		promptSnippet: "List the memories recorded, across projects.",
		exposure: "deferred",
		annotations: { readOnlyHint: true },
		description:
			"List the memories stored, with a preview of each. Reaches every project by " +
			"default; `namespace` narrows it. Read one with `mnexis_show`; `full` gives " +
			"every memory whole.",
		parameters: {
			type: "object",
			properties: {
				namespace: {
					type: "string",
					description: "Optional namespace to narrow the list.",
				},
				limit: { type: "number", description: "Maximum number of memories." },
				full: {
					type: "boolean",
					description: "Whole memories, not previews.",
				},
				type: {
					type: "string",
					description:
						"Optional: only these kinds (comma-separated, the seven in " +
						"`mnexis_save.type`).",
				},
				value: {
					type: "string",
					description:
						"Optional: only memories carrying this value — three or more digits, " +
						"exactly as written (8081 is a value, 80 is not); the answer is every " +
						"record, not a page.",
				},
			},
			additionalProperties: false,
		},
		async execute(toolCallId, params, signal, onUpdate, ctx) {
			const cli = client.withSignal(signal);
			const namespace = readNamespace(params.namespace);
			const result = await cli.list(
				{
					namespace,
					limit: params.limit,
					full: params.full,
					type: params.type,
					value: params.value,
				},
				ctx.cwd,
			);
			if (result.code !== 0) {
				return fail(describeFailure({ ...result }), {
					namespace: namespace ?? null,
				});
			}
			if (result.no_answer) {
				return fail(engineSilence(result), { namespace: namespace ?? null });
			}
			const items = readItems(result.data);
			const relations = readRelations(result.data);
			const scope = scopeLine(result.data);
			const text = items.length
				? items.map((item, index) => `${index + 1}. ${JSON.stringify(item)}`).join("\n")
				: "No memories stored yet.";
			return ok(text + relationLines(relations) + (scope ? `\n${scope}` : ""), {
				namespace: namespace ?? null,
				count: items.length,
				memories: items,
				relations,
				read_scope: readScope(result.data),
			});
		},
	});

	registerTool({
		name: "mnexis_show",
		label: "Show a memory",
		promptSnippet: "Read one stored memory by UUID.",
		annotations: { readOnlyHint: true },
		promptGuidelines: [
			"Before changing code that already exists, call mnexis_search to find what is " +
				"already recorded about it, then mnexis_show the hit you need.",
		],
		description: "Read one memory by its UUID, including its current revision and state.",
		parameters: {
			type: "object",
			properties: {
				uuid: { type: "string", description: "Memory UUID." },
			},
			required: ["uuid"],
			additionalProperties: false,
		},
		async execute(toolCallId, params, signal, onUpdate, ctx) {
			const cli = client.withSignal(signal);
			const result = await cli.show(params.uuid, ctx.cwd);
			if (result.code !== 0) {
				return fail(describeFailure({ ...result }), { uuid: params.uuid });
			}
			if (result.no_answer) {
				// The old fallback answered the string "null" here (S3, audit A8).
				return fail(engineSilence(result), { uuid: params.uuid });
			}
			return ok(result.stdout.trim() || JSON.stringify(result.data), { uuid: params.uuid, memory: result.data });
		},
	});

	registerTool({
		name: "mnexis_history",
		label: "The history of a memory",
		promptSnippet: "Every revision of one memory, and why each happened.",
		exposure: "deferred",
		annotations: { readOnlyHint: true },
		description:
			"Return one memory's revision history: every revision in order with its state, " +
			"content, who changed it and why. Use it before updating or superseding the fact.",
		parameters: {
			type: "object",
			properties: {
				uuid: { type: "string", description: "Memory UUID." },
				limit: {
					type: "number",
					description: "How many revisions to show, oldest first (default: all).",
				},
			},
			required: ["uuid"],
			additionalProperties: false,
		},
		async execute(toolCallId, params, signal, onUpdate, ctx) {
			const cli = client.withSignal(signal);
			const result = await cli.history({ uuid: params.uuid, limit: params.limit }, ctx.cwd);
			if (result.code !== 0) {
				return fail(describeFailure({ ...result }), {
					uuid: params.uuid,
				});
			}
			if (result.no_answer) {
				return fail(engineSilence(result), { uuid: params.uuid });
			}
			const items = readItems(result.data);
			return ok(result.stdout.trim() || JSON.stringify(result.data), {
				uuid: params.uuid,
				count: items.length,
				items,
			});
		},
	});

	registerTool({
		name: "mnexis_session_start",
		label: "Start the session",
		promptSnippet: "Register the session you are running in.",
		defaultActive: false,
		executionMode: "sequential",
		description:
			"Register your session, so the engine knows its boundaries. Call it once with " +
			"the session id you were given — never invented. The engine never ends a " +
			"session for you.",
		parameters: {
			type: "object",
			properties: {
				session_id: { type: "string", description: "The session id you were given." },
				directory: { type: "string", description: "Where the session runs." },
			},
			additionalProperties: false,
		},
		async execute(toolCallId, params, signal, onUpdate, ctx) {
			const sessionId = params.session_id ?? sessionIdOf(ctx);
			if (!sessionId) {
				return fail("This session has no id to register: pass `session_id`.", {
					authorized: false,
				});
			}
			const result = await client.withSignal(signal).sessionStart(
				{ sessionId, directory: params.directory ?? ctx.cwd },
				ctx.cwd,
			);
			if (result.code !== 0) {
				return fail(describeFailure({ ...result }), {
					session_id: sessionId,
				});
			}
			return ok(`Registered the session ${sessionId}.`, {
				session_id: sessionId,
				session: result.data,
			});
		},
	});

	registerTool({
		name: "mnexis_session_end",
		label: "End the session",
		promptSnippet: "End the session with a summary of what it did.",
		defaultActive: false,
		executionMode: "sequential",
		description:
			"End your session with a short summary of what it did. The next session reads it " +
			"first, so it must stand alone.",
		parameters: {
			type: "object",
			properties: {
				session_id: { type: "string", description: "The session id you were given." },
				summary: { type: "string", description: "What the session did." },
			},
			additionalProperties: false,
		},
		async execute(toolCallId, params, signal, onUpdate, ctx) {
			const sessionId = params.session_id ?? sessionIdOf(ctx);
			if (!sessionId) {
				return fail("This session has no id to end: pass `session_id`.", {
					authorized: false,
				});
			}
			const result = await client.withSignal(signal).sessionEnd(
				{ sessionId, summary: params.summary },
				ctx.cwd,
			);
			if (result.code !== 0) {
				return fail(describeFailure({ ...result }), {
					session_id: sessionId,
				});
			}
			return ok(`Ended the session ${sessionId}.`, {
				session_id: sessionId,
				session: result.data,
			});
		},
	});

	registerTool({
		name: "mnexis_save_prompt",
		label: "Save what the user said",
		promptSnippet: "Store a message from outside this session, so it survives.",
		// AB7: the `input` hook already captures every message of this
		// session, so a second writer would only duplicate rows. `hidden`
		// withdraws it from the model in pi, and the MCP surface (another
		// host, another decision) keeps its own copy.
		exposure: "hidden",
		description:
			"Store a message verbatim for the one case automatic capture misses: something " +
			"pasted from another conversation, or a statement that must be found by its " +
			"exact words. This session's messages are already stored, so do not copy the " +
			"conversation here.",
		parameters: {
			type: "object",
			properties: {
				content: { type: "string", description: "What the user said, verbatim." },
				session_id: { type: "string", description: "The session it belongs to." },
			},
			required: ["content"],
			additionalProperties: false,
		},
		async execute(toolCallId, params, signal, onUpdate, ctx) {
			const cli = client.withSignal(signal);
			const result = await cli.savePrompt(
				{ content: params.content, sessionId: params.session_id ?? sessionIdOf(ctx) },
				ctx.cwd,
			);
			if (result.code !== 0) {
				return fail(describeFailure({ ...result }), {
					authorized: false,
				});
			}
			if (result.no_answer) {
				return fail(engineSilence(result, { wrote: true }), { authorized: false });
			}
			return ok("Stored what the user said.", { prompt: result.data });
		},
	});

	registerTool({
		name: "mnexis_search_prompts",
		label: "Search what the user said",
		promptSnippet: "Find what the user asked before, across sessions.",
		exposure: "deferred",
		annotations: { readOnlyHint: true },
		description:
			"Search what the user said before, across sessions: the request behind the " +
			"work. Write the query as you would say it: every word counts, a hyphen is part " +
			"of a word.",
		parameters: {
			type: "object",
			properties: {
				query: { type: "string", description: "Search terms, in plain language." },
				match_mode: {
					type: "string",
					enum: ["any", "all", "phrase"],
					description: "How the words are matched, as in `mnexis_search`'s `match`.",
				},
				limit: { type: "number", description: "Maximum number of hits (default: 20)." },
			},
			required: ["query"],
			additionalProperties: false,
		},
		async execute(toolCallId, params, signal, onUpdate, ctx) {
			const cli = client.withSignal(signal);
			const result = await cli.searchPrompts(
				{ query: params.query, matchMode: params.match_mode, limit: params.limit },
				ctx.cwd,
			);
			if (result.code !== 0) {
				return fail(describeFailure({ ...result }), {
					authorized: false,
				});
			}
			if (result.no_answer) {
				return fail(engineSilence(result), { authorized: false });
			}
			const items = readItems(result.data);
			const text = items.length
				? items.map((item, index) => `${index + 1}. ${JSON.stringify(item)}`).join("\n")
				: "No prompts stored yet.";
			return ok(text, { count: items.length, prompts: items });
		},
	});

	registerTool({
		name: "mnexis_recall",
		label: "Recall a session",
		promptSnippet: "What a session was, in one answer.",
		exposure: "deferred",
		annotations: { readOnlyHint: true },
		description:
			"Recall a session: the session, what the user said in order, and the namespace's " +
			"memories in its window. `previous` is this workspace namespace's most recent " +
			"finished session; `session` takes an id. Memories are read by time, not by " +
			"session, each with identity and a preview; `full` gives whole records. The " +
			"answer also carries the branch state the session was anchored to, checked at " +
			"read time and never stored: `merged`, `not_merged` or `unknown`.",
		parameters: {
			type: "object",
			properties: {
				session: { type: "string", description: "The session id to recall." },
				previous: {
					type: "boolean",
					description: "The most recent finished session of the workspace's namespace.",
				},
				full: {
					type: "boolean",
					description: "Whole records, not previews.",
				},
			},
			additionalProperties: false,
		},
		async execute(toolCallId, params, signal, onUpdate, ctx) {
			// Exactly one way to ask: the engine refuses both and neither, and
			// so does the tool before anything reaches the CLI.
			if (Boolean(params.session) === Boolean(params.previous)) {
				return fail(
					"Give `session` (a known id) or `previous` (the session before this one), exactly one.",
					{ session: params.session ?? null, previous: Boolean(params.previous) },
				);
			}
			const result = await client.withSignal(signal).recall(
				{ session: params.session, previous: params.previous, full: params.full },
				ctx.cwd,
			);
			if (result.code !== 0) {
				return fail(describeFailure({ ...result }), {
					session: params.session ?? null,
					previous: Boolean(params.previous),
				});
			}
			if (result.no_answer) {
				return fail(engineSilence(result), {
					session: params.session ?? null,
					previous: Boolean(params.previous),
				});
			}
			return ok(result.stdout.trim() || JSON.stringify(result.data), {
				session_id:
					(result.data && result.data.session && result.data.session.id) ||
					params.session ||
					null,
				memories: result.data && Array.isArray(result.data.memories_in_window)
					? result.data.memories_in_window.length
					: 0,
				recall: result.data,
			});
		},
	});

	registerTool({
		name: "mnexis_workspace",
		label: "This workspace, in one answer",
		promptSnippet: "Where this session may write, and what is already there.",
		annotations: { readOnlyHint: true },
		description:
			"The state of this workspace in one answer: the namespace it writes into, the " +
			"read and write allowlists, how many memories the scope can see, and the most " +
			"recent — title and preview, never the whole record (open one with `mnexis_show`). " +
			"The decisions live here, not in the files. `mnexis_context` is the door that asks " +
			"about a task; this one answers about the workspace.",
		parameters: {
			type: "object",
			properties: {},
			additionalProperties: false,
		},
		async execute(toolCallId, params, signal, onUpdate, ctx) {
			const cli = client.withSignal(signal);
			const result = await cli.status(ctx.cwd);
			if (result.code !== 0) {
				return fail(describeFailure({ ...result }), {});
			}
			if (result.no_answer) {
				return fail(engineSilence(result), {});
			}
			return ok(result.stdout.trim() || JSON.stringify(result.data), {
				workspace: result.data && result.data.workspace ? result.data.workspace : null,
				status: result.data,
			});
		},
	});

	registerTool({
		name: "mnexis_context",
		label: "The context of this task",
		promptSnippet: "Ask the project what it knows about this task, in one call.",
		annotations: { readOnlyHint: true },
		// The start verb (AB4, bullet 1) and the way to the deferred tail
		// (bullet 4): the door is where a session begins, so it is where the
		// fork in the road is named.
		promptGuidelines: [
			"At the start of a session in an existing project, call mnexis_context once with " +
				"a one-line task summary before doing anything else.",
			"The mnexis engine has more tools than these; use tool_search to find and enable " +
				"the one a task needs.",
		],
		description:
			"Ask the project about this task in one call: the facts that match the task, " +
			"whole — the budget decides which facts fit, not how much of a fact you see — " +
			"plus what it did not include (the candidates the budget cut, with their size). " +
			"It also returns session state: the live project_state and decision records " +
			"up to the block's budget, newest first; the index of records that did not fit " +
			"(kind, title, uuid, date, no content); and the previous session's summary. That " +
			"budget is outside the door's budget, so a record it could not fit is still named. " +
			"It reads this workspace's namespace (the one a save lands in) and declares the " +
			"namespace searched, the tokens spent and how far the read reached. " +
			"`mnexis_search` is the exact query.",
		parameters: {
			type: "object",
			properties: {
				task: { type: "string", description: "The task in one line." },
				namespace: {
					type: "string",
					description: "Optional: ask another project's namespace.",
				},
				token_budget: {
					type: "number",
					description: "Token budget (default: 1024).",
				},
				max_items: {
					type: "number",
					description: "Maximum memories (default: 20, max 100).",
				},
				match: {
					type: "string",
					enum: ["any", "all", "phrase"],
					description: "How the task's words are matched, as in `mnexis_search`'s `match`.",
				},
			},
			required: ["task"],
			additionalProperties: false,
		},
		async execute(toolCallId, params, signal, onUpdate, ctx) {
			const cli = client.withSignal(signal);
			// The engine's own resolution of this workspace's namespace, read
			// from the status answer: a read, never a binding. The door must
			// not answer from another project, and the engine is the one that
			// decides which project this is. A status the engine did not answer
			// is a hint this call did without; the door's own answer below is
			// what has to arrive.
			const status = await cli.status(ctx.cwd);
			const engineNamespace =
				status.code === 0 &&
				status.data &&
				status.data.workspace &&
				typeof status.data.workspace.namespace === "string" &&
				status.data.workspace.namespace !== ""
					? status.data.workspace.namespace
					: null;
			const namespace = readNamespace(params.namespace) ?? engineNamespace ?? undefined;
			const result = await cli.context(
				{
					task: params.task,
					namespace,
					tokenBudget: params.token_budget,
					maxItems: params.max_items,
					match: params.match,
				},
				ctx.cwd,
			);
			if (result.code !== 0) {
				return fail(describeFailure({ ...result, namespace }), {
					namespace: namespace ?? null,
				});
			}
			if (result.no_answer) {
				return fail(engineSilence(result), { namespace: namespace ?? null });
			}
			// The answer hands back uuids and a left-out count: the two tools
			// that read them are activated by the call that implied them (AB2).
			activateTools(pi, IMPLIED_TAIL_TOOLS.mnexis_context);
			const items = readItems(result.data);
			// What the answer did not include: the candidates the budget
			// could not keep. Optional and additive: an engine that does
			// not declare it leaves the answer exactly as it always was.
			const coverage = readCoverage(result.data);
			// The block the engine rides along with the items. Optional, and
			// additive: an engine that does not send it, and a block with
			// nothing to say, leave the answer exactly as it always was.
			const state = readSessionState(result.data);
			// What the engine says it searched — not what we asked for: the
			// engine is the one that decided.
			const searched = searchedNamespace(result.data) ?? namespace ?? null;
			const scope = scopeLine(result.data);
			const tokens =
				result.data && typeof result.data.tokens_used === "number"
					? result.data.tokens_used
					: null;
			const text = items.length
				? items.map((item, index) => `${index + 1}. ${JSON.stringify(item)}`).join("\n")
				: `No memories answered the task '${params.task ?? ""}'.`;
			const declaration = [
				`Namespace searched: ${searched ?? "unknown"}.`,
				tokens === null ? "" : `Tokens: ${tokens}.`,
				scope,
			]
				.filter(Boolean)
				.join(" ");
			// The order is the contract: the items, what it left out, the
			// block, the declaration.
			const answer = [
				text,
				...coverageLines(coverage),
				...(state ? stateBlockLines(state) : []),
			].join("\n");
			return ok(`${answer}\n${declaration}`, {
				namespace: namespace ?? null,
				searched_namespace: searched,
				count: items.length,
				tokens_used: tokens,
				memories: items,
				read_scope: readScope(result.data),
				...(coverage ? { coverage } : {}),
				...(state ? { session_state: state } : {}),
			});
		},
	});

	registerTool({
		name: "mnexis_supersede",
		label: "Supersede a memory",
		promptSnippet: "Replace an outdated memory with a newer one, keeping the history.",
		exposure: "deferred",
		executionMode: "sequential",
		description:
			"Replace an outdated memory with a newer one, keeping the history. Use it when a " +
			"fact changed: save the new one first, then link both here. Both must live in " +
			"this project's namespace; another project's memory is refused.",
		parameters: {
			type: "object",
			properties: {
				older: { type: "string", description: "UUID of the outdated memory." },
				newer: { type: "string", description: "UUID of the replacing memory." },
				reason: { type: "string", description: "Why the newer fact replaces the older one." },
			},
			required: ["older", "newer", "reason"],
			additionalProperties: false,
		},
		async execute(toolCallId, params, signal, onUpdate, ctx) {
			const cli = client.withSignal(signal);
			const { workspace, gate } = await prepareWrite(cli, ctx);
			if (!gate.authorized) {
				return fail(gate.message, { namespace: workspace.namespace, authorized: false });
			}
			const boundary = await checkProjectBoundary(
				cli,
				[params.older, params.newer],
				workspace.namespace,
				ctx.cwd,
			);
			if (boundary) {
				return fail(boundary.message, {
					namespace: workspace.namespace,
					authorized: false,
					...boundary.details,
				});
			}
			const result = await cli.supersede(
				{ older: params.older, newer: params.newer, reason: params.reason },
				ctx.cwd,
			);
			if (result.code !== 0) {
				return fail(
					describeFailure({ ...result, namespace: workspace.namespace }),
					{ namespace: workspace.namespace },
				);
			}
			return ok(`Superseded ${params.older} with ${params.newer}.`, {
				namespace: workspace.namespace,
				output: result.stdout.trim(),
			});
		},
	});
}
