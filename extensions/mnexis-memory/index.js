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

import { createClient, defaultExec } from "./lib/cli.js";
import {
	decideAuthorization,
	decideBinding,
	describeFailure,
	allowNamespaceCommand,
	bindCommand,
} from "./lib/gate.js";
import {
	candidateFromMarker,
	deriveWorkspaceCandidate,
	findMarkerNamespace,
	readWorkspaceCandidate,
	selectNamespace,
	workspaceKey,
} from "./lib/workspace.js";
import { renderers, withStatus } from "./lib/chrome.js";

/**
 * Register a memory tool with the chrome pi draws: the call line, the panel
 * that names the answer, and the status the footer carries while it runs.
 *
 * The tool's own behaviour is untouched: the text the model reads is exactly
 * what the CLI answered, and the drawing is pi's, on the human's side.
 */
function registerMemoryTool(pi, definition) {
	pi.registerTool({
		...definition,
		...renderers(definition.name),
		execute: withStatus(definition.name, definition.execute),
	});
}

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
		if (result.code !== 0) {
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
 * True when `item` could be the live twin of the record a save just wrote,
 * judged on the identity a ranked hit already answers (S2.1, S2.3).
 *
 * The cheap half of the rule, deliberately: a search hit carries the uuid,
 * the state, the namespace and the words, so everything except the numeric
 * signature and the summary test is decided before a record is read whole. A
 * superseded row is history, not a second live record; a record in another
 * namespace is another project's; no shared keyword is another fact that
 * happens to carry the same number (L2).
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
 * Every live twin the candidate query answered, as pairs (S1, B3).
 *
 * All of them: S1 promises the fact ends with exactly one live record, and
 * retiring one of N twins delivers "one fewer", not "one". The order is the
 * engine's own id order, which is the order a reader expects the older
 * records in.
 */
export function twinPairs(saved, candidates, namespace) {
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
	return pairs;
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

export default function mnexisMemoryExtension(pi) {
	// One client and one resolution cache per extension instance (per session).
	const client = createClient({
		exec: defaultExec,
		binary: process.env.MNEXIS_BIN,
		env: process.env,
	});
	let resolution = null;
	let writeNamespaces = null;

	/** Resolve the workspace → namespace binding once per session. */
	async function resolveWorkspace(cwd) {
		if (resolution) {
			return resolution;
		}
		const root = await client.git(["rev-parse", "--show-toplevel"], cwd);
		const repoRoot = root.code === 0 && root.value ? root.value : null;
		const remote = await client.git(["config", "--get", "remote.origin.url"], repoRoot || cwd);
		const remoteUrl = remote.code === 0 && remote.value ? remote.value : null;
		const key = workspaceKey({ remote: remoteUrl, repoRoot, cwd });
		const rootPath = repoRoot || cwd;

		const list = await client.projectList(cwd);
		const bindings = list.code === 0 && list.data && Array.isArray(list.data.bindings) ? list.data.bindings : [];
		// The engine owns the identity rule: the registry publishes the
		// canonical key of the workspace this process runs in, so the
		// binding the human recorded and the one the agent looks up are
		// the same key. The computed key stays as the fallback for an
		// older engine that does not publish it yet.
		const publishedKey =
			list.code === 0 && list.data && typeof list.data.current_key === "string" && list.data.current_key !== ""
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
				const status = await client.status(cwd);
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

	/** Read the profile allowlist once per session, initializing it if needed. */
	async function getWriteNamespaces(cwd) {
		if (writeNamespaces) {
			return writeNamespaces;
		}
		let config = await client.configShow(cwd);
		if (config.code !== 0) {
			throw new Error(describeFailure({ code: config.code, stderr: config.stderr }));
		}
		// `mnexis project bind` works without a profile, so the binding can
		// exist while the profile does not. Every write needs the profile.
		if (needsProfileInit(config.data)) {
			const init = await client.init(cwd);
			if (init.code !== 0) {
				throw new Error(describeFailure({ code: init.code, stderr: init.stderr }));
			}
			config = await client.configShow(cwd);
			if (config.code !== 0) {
				throw new Error(describeFailure({ code: config.code, stderr: config.stderr }));
			}
		}
		writeNamespaces = readWriteNamespaces(config.data);
		return writeNamespaces;
	}

	/**
	 * Ensure the namespace may be written to, asking the human once.
	 *
	 * Interactive sessions confirm through pi's UI; headless sessions never
	 * authorize anything and report the exact command a human must run.
	 */
	async function ensureWritable(namespace, ctx) {
		const allowed = await getWriteNamespaces(ctx.cwd);
		const decision = decideAuthorization({
			namespace,
			writeNamespaces: allowed,
			hasUI: Boolean(ctx.hasUI),
			mode: ctx.mode,
		});
		if (decision.action === "allow") {
			return { authorized: true, asked: false };
		}
		if (decision.action === "pending") {
			return {
				authorized: false,
				asked: false,
				message:
					`Writing to namespace '${namespace}' needs your authorization first. ` +
					`Run \`${allowNamespaceCommand(namespace)}\` once, then retry. ` +
					`Reason: ${decision.reason}.`,
			};
		}
		const confirmed = await ctx.ui.confirm(
			`Allow the mnexis agent to write memories in namespace '${namespace}'?\n\n` +
				"This grants write permission (save, update, supersede) for that namespace only, " +
				"in the current mnexis profile. Other namespaces and global access stay untouched, " +
				"and you can revoke it later by editing the profile allowlist.",
			"mnexis: authorize namespace",
		);
		if (!confirmed) {
			return {
				authorized: false,
				asked: true,
				message: `Authorization for namespace '${namespace}' was declined. No memory was written.`,
			};
		}
		const allow = await client.allowNamespace(namespace, ctx.cwd);
		if (allow.code !== 0) {
			return {
				authorized: false,
				asked: true,
				message: describeFailure({ code: allow.code, stderr: allow.stderr, namespace }),
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
	async function bindOnConfirmation(workspace, ctx) {
		const decision = decideBinding({
			candidate: workspace.candidate,
			refusal: workspace.refusal,
			hasUI: Boolean(ctx.hasUI),
			mode: ctx.mode,
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
		const confirmed = await ctx.ui.confirm(
			bindingPrompt(workspace),
			"mnexis: name this workspace",
		);
		if (!confirmed) {
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
		const allowed = await getWriteNamespaces(ctx.cwd);
		const bind = await client.bind(workspace.candidate, workspace.key, workspace.root, ctx.cwd);
		if (bind.code !== 0) {
			return {
				workspace,
				gate: {
					authorized: false,
					asked: true,
					message: `${describeFailure({
						code: bind.code,
						stderr: bind.stderr,
						namespace: workspace.candidate,
					})}. No memory was written.`,
				},
			};
		}
		const allow = await client.allowNamespace(workspace.candidate, ctx.cwd);
		if (allow.code !== 0) {
			return {
				workspace,
				gate: {
					authorized: false,
					asked: true,
					message:
						`The workspace was bound to '${workspace.candidate}', but the write allowlist ` +
						`could not be updated: ${describeFailure({
							code: allow.code,
							stderr: allow.stderr,
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
	async function listByValue(namespace, value, ctx) {
		const items = [];
		for (let page = 0; page < TWIN_SCAN_MAX_PAGES; page += 1) {
			const listed = await client.list(
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
						error: describeFailure({ code: listed.code, stderr: listed.stderr, namespace }),
					},
				};
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
	async function candidatesByValue(saved, ctx) {
		let candidates = null;
		for (const value of numericValues(saved.content)) {
			const answered = await listByValue(saved.namespace, value, ctx);
			if (answered.error) {
				return answered;
			}
			if (candidates === null) {
				candidates = answered.items;
				continue;
			}
			const carried = new Set(answered.items.map((item) => item.uuid));
			candidates = candidates.filter((item) => carried.has(item.uuid));
		}
		return { items: candidates ?? [], source: "value" };
	}

	/**
	 * The whole record a candidate needs to be judged (S2.2, S2.4).
	 *
	 * A `list --full` page already carries the content; a ranked hit carries
	 * the identity only, so only the records that survived the cheap half of
	 * the rule are read whole — a twin is checked against what the store says,
	 * never against a preview.
	 */
	async function readCandidate(item, ctx) {
		if (typeof item.content === "string") {
			return { record: item };
		}
		const shown = await client.show(item.uuid, ctx.cwd);
		if (shown.code !== 0) {
			return {
				error: {
					step: "show",
					uuid: item.uuid,
					error: describeFailure({
						code: shown.code,
						stderr: shown.stderr,
						namespace: item.namespace,
					}),
				},
			};
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
	async function repairSavedTwin(workspace, params, answer, ctx) {
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
		};
		try {
			const allowed = await getWriteNamespaces(ctx.cwd);
			if (!allowed.includes(namespace)) {
				return null;
			}
			const found = await candidatesByValue(saved, ctx);
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
				const resolved = await readCandidate(item, ctx);
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
			const pairs = twinPairs(saved, candidates, namespace);
			const repaired = [];
			for (const pair of pairs) {
				// S5 keeps the measured direction: the older record is
				// superseded onto the one the save wrote, which is the newer
				// record, and the engine keeps the row and the link.
				const superseded = await client.supersede(
					{ older: pair.older.uuid, newer: savedUuid, reason: twinReason(pair) },
					ctx.cwd,
				);
				if (superseded.code !== 0) {
					const failure = {
						step: "supersede",
						uuid: pair.older.uuid,
						value: pair.value,
						error: describeFailure({
							code: superseded.code,
							stderr: superseded.stderr,
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
	async function prepareWrite(ctx) {
		const workspace = await resolveWorkspace(ctx.cwd);
		if (!workspace.bound) {
			// No binding: the namespace is the human's decision. The engine's
			// candidate is the question; it is never the answer on its own.
			return bindOnConfirmation(workspace, ctx);
		}
		const gate = await ensureWritable(workspace.namespace, ctx);
		return { workspace, gate };
	}

	// The session the extension runs in: the engine knows its boundaries
	// without the model having to remember them. Both hooks are best
	// effort — a session that was never registered is not an error, it is
	// a session the engine did not hear about.
	pi.on("session_start", async (_event, ctx) => {
		const id = sessionIdOf(ctx);
		if (!id) {
			return;
		}
		try {
			await client.sessionStart({ sessionId: id, directory: ctx.cwd }, ctx.cwd);
		} catch {
			// The profile may not be initialised yet, or the namespace may
			// not be allowed for writes. The session still works.
		}
	});

	pi.on("session_shutdown", async (_event, ctx) => {
		const id = sessionIdOf(ctx);
		if (!id) {
			return;
		}
		try {
			await client.sessionEnd({ sessionId: id }, ctx.cwd);
		} catch {
			// Same as above: the end of a session is not worth an error.
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
	pi.on("input", async (event, ctx) => {
		const text = typeof event?.text === "string" ? event.text : "";
		// A message another extension sent is not something the human said.
		if (text === "" || event?.source === "extension") {
			return;
		}
		let result;
		try {
			result = await client.savePrompt(
				{ content: text, sessionId: sessionIdOf(ctx) },
				ctx.cwd,
			);
		} catch (error) {
			journalPromptSkip(ctx, {
				bytes: Buffer.byteLength(text, "utf8"),
				error: error instanceof Error ? error.message : String(error),
			});
			return;
		}
		if (result.code !== 0) {
			journalPromptSkip(ctx, {
				bytes: Buffer.byteLength(text, "utf8"),
				exit_code: result.code,
			});
		}
	});

	// Where a compaction lands. The summary of the compacted context is
	// what the session was doing; storing it means the next session can
	// recover it from the memory instead of from a context that no longer
	// exists. Only where the human already allowed writes.
	pi.on("session_compact", async (event, ctx) => {
		const summary = event?.compactionEntry?.summary;
		if (typeof summary !== "string" || summary.trim() === "") {
			return;
		}
		try {
			const workspace = await resolveWorkspace(ctx.cwd);
			// Only a bound workspace has a namespace that is a permission: a
			// marker names a candidate, and a file inside the workspace must
			// not become the target of a write on its own (auto-namespace S6).
			if (!workspace.bound || !workspace.namespace) {
				return;
			}
			const allowed = await getWriteNamespaces(ctx.cwd);
			if (!allowed.includes(workspace.namespace)) {
				return;
			}
			const id = sessionIdOf(ctx);
			await client.save(
				{
					namespace: workspace.namespace,
					content:
						`Session ${id ?? "unknown"} was compacted. What was happening:\n\n${summary}`,
					tags: ["compaction"],
				},
				ctx.cwd,
			);
		} catch {
			// A compaction must not fail because the memory could not take it.
		}
	});

	// The gate lives in this extension, so a model-issued call must not
	// grant the permissions behind its back. The human's own commands are
	// untouched: only model tool calls are inspected here.
	pi.on("tool_call", async (event) => {
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

	registerMemoryTool(pi, {
		name: "mnexis_save",
		label: "Save a project memory",
		promptSnippet: "Store a durable memory of this project in the local mnexis engine.",
		description:
			"Store a durable memory about this project so future sessions can recall it. " +
			"Use it after a decision, a discovery, a milestone or a stated preference — " +
			"content must make sense months later without the surrounding conversation. " +
			"The namespace is resolved from the current workspace automatically. " +
			"Give it a title: the title is what the fact IS, the content is what it says " +
			"now, and the title decides which record the memory lands on. The same title " +
			"always means the same record, so a fact that changed updates its record " +
			"instead of adding a second one. Give it keywords too: the words the same fact " +
			"is called by in another language or another team's jargon, so it is found " +
			"when it is asked for that way. They are the topic's vocabulary, never a value.",
		parameters: {
			type: "object",
			properties: {
				content: { type: "string", description: "One self-contained sentence." },
				title: {
					type: "string",
					description:
						"Short, stable label of the fact (what it IS, not what it says now). " +
						"The same title always lands on the same record.",
				},
				tags: { type: "string", description: "Comma-separated tags." },
				keywords: {
					type: "string",
					description:
						"Comma-separated words the fact is called by in another language or " +
						"another team's jargon (e.g. `puerto, port, listen port`). They are the " +
						"topic's vocabulary, never a value, and they accumulate across saves.",
				},
				topic_key: { type: "string", description: "Stable key grouping facts about one topic." },
				type: {
					type: "string",
					description:
						"Optional: the kind of thing this fact is, one of `knowledge`, `decision`, " +
						"`preference`, `procedure`, `project_state`, `episode`, `fact`. Without it " +
						"the engine decides.",
				},
				idempotency_key: { type: "string", description: "Makes retries safe." },
			},
			required: ["content"],
			additionalProperties: false,
		},
		async execute(toolCallId, params, signal, onUpdate, ctx) {
			const { workspace, gate } = await prepareWrite(ctx);
			if (!gate.authorized) {
				return fail(gate.message, { namespace: workspace.namespace, authorized: false });
			}
			const result = await client.save(
				{
					namespace: workspace.namespace,
					content: params.content,
					title: params.title,
					tags: params.tags,
					keywords: params.keywords,
					topicKey: params.topic_key,
					idempotencyKey: params.idempotency_key,
					type: params.type,
				},
				ctx.cwd,
			);
			if (result.code !== 0) {
				return fail(
					describeFailure({ code: result.code, stderr: result.stderr, namespace: workspace.namespace }),
					{ namespace: workspace.namespace },
				);
			}
			// The same title lands on the same record, so a save can update
			// a fact instead of adding a second one. The model has to read
			// that it happened.
			const revision = result.data && result.data.revision;
			const topicKey = result.data && result.data.topic_key;
			const what =
				typeof revision === "number" && revision > 1
					? `Updated the memory in namespace '${workspace.namespace}' (revision ${revision}, topic key \`${topicKey}\`).`
					: `Saved a memory in namespace '${workspace.namespace}'.`;
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
			const repair = await repairSavedTwin(workspace, params, result.data, ctx);
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

	registerMemoryTool(pi, {
		name: "mnexis_search",
		label: "Search memories",
		promptSnippet: "Recall what was recorded, across projects.",
		description:
			"Search stored memories (full-text, ranked). Use it before answering " +
			"from scratch when prior context may exist, and to check what was already recorded. " +
			"The search reaches every project by default, because projects share components; " +
			"pass `namespace` to narrow it to one project. " +
			"Write the query the way you would say it: every word counts and a hyphen is " +
			"part of a word, so `staging-eu` and `puerto base de datos` both work.",
		parameters: {
			type: "object",
			properties: {
				query: { type: "string", description: "Search terms, in plain language." },
				limit: { type: "number", description: "Maximum number of hits (default: 10)." },
				namespace: {
					type: "string",
					description: "Optional namespace to narrow the search to one project.",
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
						"Optional: keep only the memories of these kinds — a comma-separated list of " +
						"`knowledge`, `decision`, `preference`, `procedure`, `project_state`, " +
						"`episode`, `fact`.",
				},
			},
			required: ["query"],
			additionalProperties: false,
		},
		async execute(toolCallId, params, signal, onUpdate, ctx) {
			const namespace = readNamespace(params.namespace);
			const result = await client.search(
				{ query: params.query, namespace, limit: params.limit, match: params.match, type: params.type },
				ctx.cwd,
			);
			if (result.code !== 0) {
				return fail(describeFailure({ code: result.code, stderr: result.stderr }), {
					namespace: namespace ?? null,
				});
			}
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

	registerMemoryTool(pi, {
		name: "mnexis_suggest_topic_key",
		label: "Suggest the topic key",
		promptSnippet: "Ask which topic key a fact is already recorded under.",
		description:
			"Ask which topic key a title yields, and whether this project already records " +
			"it. Use it before saving a fact you are not sure about: the same title always " +
			"lands on the same record, so a fact that changed updates its record instead of " +
			"adding a second one.",
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
			const workspace = await resolveWorkspace(ctx.cwd);
			const namespace = readNamespace(params.namespace) ?? workspace.namespace;
			const result = await client.suggestTopicKey({ title: params.title, namespace }, ctx.cwd);
			if (result.code !== 0) {
				return fail(describeFailure({ code: result.code, stderr: result.stderr, namespace }), {
					namespace: namespace ?? null,
				});
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

	registerMemoryTool(pi, {
		name: "mnexis_list",
		label: "List memories",
		promptSnippet: "List the memories recorded, across projects.",
		description:
			"List the memories stored: what is there, with a preview " +
			"of each one. Use it to review what is already recorded before saving " +
			"something new. The list reaches every project by default; pass `namespace` " +
			"to narrow it to one project. Each item names the memory and shows a few " +
			"words of it: read the one that matters with `mnexis_show`, and ask for " +
			"`full` only when you need the whole text of every memory.",
		parameters: {
			type: "object",
			properties: {
				namespace: {
					type: "string",
					description: "Optional namespace to narrow the list to one project.",
				},
				limit: { type: "number", description: "Maximum number of memories." },
				full: {
					type: "boolean",
					description: "The complete content of every memory, not a preview.",
				},
				type: {
					type: "string",
					description:
						"Optional: keep only the memories of these kinds — a comma-separated list of " +
						"`knowledge`, `decision`, `preference`, `procedure`, `project_state`, " +
						"`episode`, `fact`.",
				},
				value: {
					type: "string",
					description:
						"Optional: keep only the memories that carry this value — a run of three or " +
						"more digits, exactly as written (8081 is a value, 80 is not). The answer is " +
						"every record that carries it, not a page of the namespace.",
				},
			},
			additionalProperties: false,
		},
		async execute(toolCallId, params, signal, onUpdate, ctx) {
			const namespace = readNamespace(params.namespace);
			const result = await client.list(
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
				return fail(describeFailure({ code: result.code, stderr: result.stderr }), {
					namespace: namespace ?? null,
				});
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

	registerMemoryTool(pi, {
		name: "mnexis_show",
		label: "Show a memory",
		promptSnippet: "Read one stored memory by UUID.",
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
			const result = await client.show(params.uuid, ctx.cwd);
			if (result.code !== 0) {
				return fail(describeFailure({ code: result.code, stderr: result.stderr }), { uuid: params.uuid });
			}
			return ok(result.stdout.trim() || JSON.stringify(result.data), { uuid: params.uuid, memory: result.data });
		},
	});

	registerMemoryTool(pi, {
		name: "mnexis_history",
		label: "The history of a memory",
		promptSnippet: "Every revision of one memory, and why each happened.",
		description:
			"Return the revision history of one memory: every revision in order, with its state, " +
			"content, who changed it and why. Use it to see how a fact evolved before updating or " +
			"superseding it — the current record alone does not say what it used to claim.",
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
			const result = await client.history({ uuid: params.uuid, limit: params.limit }, ctx.cwd);
			if (result.code !== 0) {
				return fail(describeFailure({ code: result.code, stderr: result.stderr }), {
					uuid: params.uuid,
				});
			}
			const items = readItems(result.data);
			return ok(result.stdout.trim() || JSON.stringify(result.data), {
				uuid: params.uuid,
				count: items.length,
				items,
			});
		},
	});

	registerMemoryTool(pi, {
		name: "mnexis_session_start",
		label: "Start the session",
		promptSnippet: "Register the session you are running in.",
		description:
			"Register the session you are running in, so the engine knows its boundaries. Call it " +
			"once, at the start of the session: the session id is yours (the one you were given), " +
			"never invented. The engine never ends a session for you.",
		parameters: {
			type: "object",
			properties: {
				session_id: { type: "string", description: "The session id you were given." },
				directory: { type: "string", description: "Where the session runs (informational)." },
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
			const result = await client.sessionStart(
				{ sessionId, directory: params.directory ?? ctx.cwd },
				ctx.cwd,
			);
			if (result.code !== 0) {
				return fail(describeFailure({ code: result.code, stderr: result.stderr }), {
					session_id: sessionId,
				});
			}
			return ok(`Registered the session ${sessionId}.`, {
				session_id: sessionId,
				session: result.data,
			});
		},
	});

	registerMemoryTool(pi, {
		name: "mnexis_session_end",
		label: "End the session",
		promptSnippet: "End the session with a summary of what it did.",
		description:
			"End the session you are running in, with a short summary of what it did. The summary " +
			"is what the next session reads first, so it must make sense with no surrounding chat.",
		parameters: {
			type: "object",
			properties: {
				session_id: { type: "string", description: "The session id you were given." },
				summary: { type: "string", description: "What the session did, in a few sentences." },
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
			const result = await client.sessionEnd(
				{ sessionId, summary: params.summary },
				ctx.cwd,
			);
			if (result.code !== 0) {
				return fail(describeFailure({ code: result.code, stderr: result.stderr }), {
					session_id: sessionId,
				});
			}
			return ok(`Ended the session ${sessionId}.`, {
				session_id: sessionId,
				session: result.data,
			});
		},
	});

	registerMemoryTool(pi, {
		name: "mnexis_save_prompt",
		label: "Save what the user said",
		promptSnippet: "Store a message from outside this session, so it survives.",
		description:
			"Store a message verbatim, for the one case the automatic capture does not cover: " +
			"something the user pasted from another conversation, or a statement that must be found " +
			"again by its exact words. Every message of this session is already stored when it " +
			"arrives, by the extension and not by you, so do not copy the conversation here.",
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
			const result = await client.savePrompt(
				{ content: params.content, sessionId: params.session_id ?? sessionIdOf(ctx) },
				ctx.cwd,
			);
			if (result.code !== 0) {
				return fail(describeFailure({ code: result.code, stderr: result.stderr }), {
					authorized: false,
				});
			}
			return ok("Stored what the user said.", { prompt: result.data });
		},
	});

	registerMemoryTool(pi, {
		name: "mnexis_search_prompts",
		label: "Search what the user said",
		promptSnippet: "Find what the user asked before, across sessions.",
		description:
			"Search what the user said before, across sessions. Use it when you need the request " +
			"behind the work: the prompt is not in the files. Write the query the way you would " +
			"say it: every word counts and a hyphen is part of a word.",
		parameters: {
			type: "object",
			properties: {
				query: { type: "string", description: "Search terms, in plain language." },
				match_mode: {
					type: "string",
					enum: ["any", "all", "phrase"],
					description:
						"How the words are matched: any (default) counts every word, all requires " +
						"every word, phrase keeps the query as one exact phrase.",
				},
				limit: { type: "number", description: "Maximum number of hits (default: 20)." },
			},
			required: ["query"],
			additionalProperties: false,
		},
		async execute(toolCallId, params, signal, onUpdate, ctx) {
			const result = await client.searchPrompts(
				{ query: params.query, matchMode: params.match_mode, limit: params.limit },
				ctx.cwd,
			);
			if (result.code !== 0) {
				return fail(describeFailure({ code: result.code, stderr: result.stderr }), {
					authorized: false,
				});
			}
			const items = readItems(result.data);
			const text = items.length
				? items.map((item, index) => `${index + 1}. ${JSON.stringify(item)}`).join("\n")
				: "No prompts stored yet.";
			return ok(text, { count: items.length, prompts: items });
		},
	});

	registerMemoryTool(pi, {
		name: "mnexis_recall",
		label: "Recall a session",
		promptSnippet: "What a session was, in one answer.",
		description:
			"Recall what a session was: the session itself, what the user said in it in order, and " +
			"the memories of the namespace in its window. Use `previous` for the session before this " +
			"one — the most recent finished session of this workspace's namespace — or `session` for a " +
			"known id. The memories are the namespace's, read by time: the engine does not know which " +
			"session wrote which memory. Each memory is answered with its identity and a preview; pass " +
			"`full` for the whole records, and read one with `mnexis_show`. The answer also carries the " +
			"state of the branch the session was anchored to, validated at read time and never stored: " +
			"`merged`, `not_merged`, or `unknown` when it could not be checked.",
		parameters: {
			type: "object",
			properties: {
				session: { type: "string", description: "The session id to recall." },
				previous: {
					type: "boolean",
					description: "The session before this one (the most recent finished session of the workspace's namespace).",
				},
				full: {
					type: "boolean",
					description: "The complete content of every memory in the window, not a preview.",
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
			const result = await client.recall(
				{ session: params.session, previous: params.previous, full: params.full },
				ctx.cwd,
			);
			if (result.code !== 0) {
				return fail(describeFailure({ code: result.code, stderr: result.stderr }), {
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

	registerMemoryTool(pi, {
		name: "mnexis_workspace",
		label: "This workspace, in one answer",
		promptSnippet: "Where this session may write, and what is already there.",
		description:
			"Call this FIRST, at the start of every session and before you change code that already " +
			"exists: it returns the namespace this workspace writes into, the read and write " +
			"allowlists, how many memories the scope can see, and the most recent of them — each " +
			"with its title and a preview, never the whole record (open one with mnexis_show when " +
			"it is the memory you need). The decisions behind the code live here, not in the files.",
		parameters: {
			type: "object",
			properties: {},
			additionalProperties: false,
		},
		async execute(toolCallId, params, signal, onUpdate, ctx) {
			const result = await client.status(ctx.cwd);
			if (result.code !== 0) {
				return fail(describeFailure({ code: result.code, stderr: result.stderr }), {});
			}
			return ok(result.stdout.trim() || JSON.stringify(result.data), {
				workspace: result.data && result.data.workspace ? result.data.workspace : null,
				status: result.data,
			});
		},
	});

	registerMemoryTool(pi, {
		name: "mnexis_context",
		label: "The context of this task",
		promptSnippet: "Ask the project what it knows about this task, in one call.",
		description:
			"Ask the project a question in one call and get the answer: the facts " +
			"this project recorded that match the task, whole — the uuid, the title " +
			"and the entire content of each memory, never a cut one, because the " +
			"budget decides which facts fit, not how much of a fact you see — PLUS " +
			"the declaration of what the answer did not include: the candidates the " +
			"budget could not keep, with their size, so a cross-check round has " +
			"something to look for, and a fact that did not fit is one " +
			"mnexis_show away. It also carries the " +
			"session state — the live project_state and decision " +
			"records with the content that fits the block's budget, newest first, " +
			"the index of the live records that did not fit (kind, title, uuid and " +
			"date, no content — pull one with mnexis_show), and the previous " +
			"session's summary — so one call answers 'where did we leave off' " +
			"instead of rebuilding it from several surfaces. The block has a budget " +
			"of its own and is outside the door's budget, so it never moves which " +
			"memories the door picks; a record the block could not fit is still " +
			"named, never dropped. Use it as the FIRST call of a session, before " +
			"you read the code, and whenever the task is shaped like a question " +
			"('where did we leave off', 'which port does it listen on', 'what did " +
			"we decide about X'). Read the session state it returns before calling " +
			"list, search or recall for the same question: the state is already " +
			"here. It searches this workspace's namespace — the one a save lands in " +
			"— and the answer declares the namespace it searched, the tokens it " +
			"spent and how far the read reached, so an empty answer is never " +
			"another project's silence. It does not replace mnexis_search: the " +
			"search is the exact query, the context is the project-shaped question.",
		parameters: {
			type: "object",
			properties: {
				task: { type: "string", description: "The task in one line." },
				namespace: {
					type: "string",
					description:
						"Optional namespace to ask another project. Defaults to this workspace's.",
				},
				token_budget: {
					type: "number",
					description: "Token budget for the answer (default: 1024).",
				},
				max_items: {
					type: "number",
					description: "Maximum number of memories to consider (default: 20, capped at 100).",
				},
				match: {
					type: "string",
					enum: ["any", "all", "phrase"],
					description:
						"How the task's words are matched: any (default) counts every word, all " +
						"requires every word, phrase keeps the task as one exact phrase.",
				},
			},
			required: ["task"],
			additionalProperties: false,
		},
		async execute(toolCallId, params, signal, onUpdate, ctx) {
			// The engine's own resolution of this workspace's namespace, read
			// from the status answer: a read, never a binding. The door must
			// not answer from another project, and the engine is the one that
			// decides which project this is.
			const status = await client.status(ctx.cwd);
			const engineNamespace =
				status.code === 0 &&
				status.data &&
				status.data.workspace &&
				typeof status.data.workspace.namespace === "string" &&
				status.data.workspace.namespace !== ""
					? status.data.workspace.namespace
					: null;
			const namespace = readNamespace(params.namespace) ?? engineNamespace ?? undefined;
			const result = await client.context(
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
				return fail(describeFailure({ code: result.code, stderr: result.stderr, namespace }), {
					namespace: namespace ?? null,
				});
			}
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

	registerMemoryTool(pi, {
		name: "mnexis_supersede",
		label: "Supersede a memory",
		promptSnippet: "Replace an outdated memory with a newer one, keeping the history.",
		description:
			"Replace an outdated memory with a newer one, keeping the history. Use it when a fact " +
			"changed: save the new fact first, then link both records with this tool. Both memories " +
			"must live in this project's namespace: the tool refuses to touch a memory that belongs " +
			"to another project.",
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
			const { workspace, gate } = await prepareWrite(ctx);
			if (!gate.authorized) {
				return fail(gate.message, { namespace: workspace.namespace, authorized: false });
			}
			const boundary = await checkProjectBoundary(
				client,
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
			const result = await client.supersede(
				{ older: params.older, newer: params.newer, reason: params.reason },
				ctx.cwd,
			);
			if (result.code !== 0) {
				return fail(
					describeFailure({ code: result.code, stderr: result.stderr, namespace: workspace.namespace }),
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
