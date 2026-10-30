/**
 * Store recovery on `mnexis_save` (odd/tasks/store-recovery.md, S1-S7).
 *
 * A save that would leave two live records for one fact is repaired by
 * superseding every older twin onto the one the save wrote. Every test drives
 * the tool the way pi does and reads the store the fake engine keeps, so the
 * assertions are about the store and the answer, never about the helpers that
 * implement the rule.
 *
 * Each rule test carries a control: the same corpus with the rule satisfied
 * must be repaired, and the boundary case must not be touched. Without the
 * control a negative case would pass before the feature exists, and a test
 * that cannot fail proves nothing.
 *
 * THE FAKE MIRRORS THE ENGINE (this is what the first version of this corpus
 * got wrong, see L4). It reproduces, for every behaviour these tests depend
 * on, what `mnexis` 0.5.0-dev actually does:
 *
 *   - `list` orders by id and applies the limit AFTER the filters
 *     (`src/storage/memory_repo.rs:99-104`): OLDEST first, so a page of a
 *     namespace is not "the recent records".
 *   - `list --value <N>` filters by the value BEFORE the limit, with the
 *     engine's own rule: a MAXIMAL digit run of the record's indexed text
 *     (content, title, topic key, keywords) exactly equal to the value
 *     (`src/domain/value.rs`). A value that is not three or more digits is an
 *     INVALID_ARGUMENT (exit 2), refused with the engine's own message.
 *   - `history` answers `count: 0` / "no history for <uuid>" when no
 *     `memory_history` row exists. A row exists only after a change: the
 *     engine records the state the change replaced, on the update path (a
 *     save on a title the store already holds) and on the supersede path
 *     (`src/storage/memory_repo.rs`, `update_with_history_and_tags` and
 *     `supersede_atomic`). A first save writes no row.
 *   - `supersede` fails the way the engine fails: unknown uuid (NOT_FOUND,
 *     exit 3), cross-namespace and a superseded older record
 *     (INVALID_ARGUMENT, exit 2), with the engine's own messages.
 *   - `search` is ranked, active-only, namespace-filtered, answers hits
 *     without content, and caps at the engine's `MAX_LIMIT` (100). bm25 is not
 *     modelled: the fake ranks shorter content first, which is the direction
 *     bm25 takes, and then by id. The order only decides what a saturated page
 *     cuts.
 */

import { test } from "node:test";
import assert from "node:assert/strict";
import { chmodSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import mnexisMemory from "../index.js";

const NAMESPACE = "payments";
const OTHER_NAMESPACE = "otro-proyecto";

/** A fake `mnexis` whose store lives in a JSON file the test can read. */
const FAKE_BINARY = `#!/usr/bin/env node
import { appendFileSync, readFileSync, writeFileSync } from "node:fs";

const log = process.env.MNEXIS_FAKE_LOG;
const configPath = process.env.MNEXIS_FAKE_CONFIG;
const args = process.argv.slice(2);
appendFileSync(log, JSON.stringify(args) + "\\n");
const config = JSON.parse(readFileSync(configPath, "utf8"));
config.store = config.store ?? [];
config.history = config.history ?? [];
config.links = config.links ?? [];
const persist = () => writeFileSync(configPath, JSON.stringify(config));

function flag(name) {
	const index = args.indexOf(name);
	return index >= 0 ? args[index + 1] : undefined;
}

const NOT_FOUND = 3;
const INVALID_ARGUMENT = 2;

/**
 * The engine answers a JSON error on stderr and a non-zero exit code; the
 * extension's describeFailure reads that stderr.
 */
function refuse(code, errorCode, message) {
	process.stderr.write(JSON.stringify({ error: { error_code: errorCode, message } }));
	persist();
	process.exit(code);
}

/** \`key_from_title\` (src/domain/title.rs:26): the identity of a fact. */
function keyFromTitle(title) {
	let out = "";
	for (const char of String(title ?? "").trim()) {
		const lowered = char.toLowerCase();
		if (/^[\\p{L}\\p{N}]+$/u.test(lowered)) {
			out += lowered;
		} else if (out !== "" && !out.endsWith("-")) {
			out += "-";
		}
	}
	return out.replace(/^-+/, "").replace(/-+$/, "");
}

/** \`normalize_keywords\`: trimmed, empty dropped, the first spelling wins. */
function normalizeKeywords(words) {
	const seen = new Set();
	const out = [];
	for (const word of words) {
		const trimmed = String(word).trim();
		if (trimmed === "") {
			continue;
		}
		const key = trimmed.toLowerCase();
		if (seen.has(key)) {
			continue;
		}
		seen.add(key);
		out.push(trimmed);
	}
	return out;
}

const isActive = (item) => item.deleted !== true && item.state === "active";
const byId = (left, right) => left.id - right.id;
const stamp = (id) => "2026-10-28T00:00:00Z".replace("00:00", String(10 + (id % 40)).padStart(2, "0") + ":" + String(id % 60).padStart(2, "0"));

/** The tokens the FTS index holds for a record (content, title, topic, words). */
function indexedTokens(item) {
	const text = [item.content, item.title, item.topic_key, ...(item.keywords ?? [])].join(" ");
	return new Set(text.toLowerCase().split(/[^\\p{L}\\p{N}]+/u).filter(Boolean));
}

/**
 * The engine's value filter (src/domain/value.rs): a record carries the
 * value when one of its MAXIMAL digit runs, in its indexed text, is the
 * value exactly. A run of one or two digits is not a value, and a longer run
 * (\`80810\`) is a different one.
 */
function carriesValue(item, value) {
	const text = [item.content, item.title, item.topic_key, ...(item.keywords ?? [])].join(" ");
	return (String(text).match(/\\d+/g) ?? []).some(
		(run) => run.length >= 3 && run === value,
	);
}

if (args[0] === "project" && args[1] === "list") {
	// The published key is the one the extension uses: no git derivation
	// enters this corpus.
	const bindings = config.bound === false ? [] : [{ key: "test-key", namespace: config.namespace }];
	process.stdout.write(JSON.stringify({ bindings, current_key: "test-key" }));
} else if (args[0] === "config" && args[1] === "show") {
	process.stdout.write(
		JSON.stringify({
			config_file_present: true,
			scope: { write_namespaces: config.writeNamespaces ?? [config.namespace] },
		}),
	);
} else if (args[0] === "status") {
	process.stdout.write(
		JSON.stringify({
			workspace: {
				key: "test-key",
				namespace: config.bound === false ? null : config.namespace,
				candidate_namespace: config.candidate ?? null,
				candidate_refusal: config.candidate_refusal ?? null,
			},
		}),
	);
} else if (args[0] === "save") {
	const namespace = flag("--namespace") ?? config.namespace;
	const end = args.indexOf("--json");
	const content = args[end - 1];
	const title = flag("--title") ?? "";
	const keywords = normalizeKeywords((flag("--keywords") ?? "").split(","));
	const topicKey = (flag("--topic-key") ?? "") || (title.trim() === "" ? "" : keyFromTitle(title));
	const occupier =
		topicKey === ""
			? undefined
			: config.store.find(
					(item) => item.namespace === namespace && item.topic_key === topicKey,
				);
	if (occupier) {
		if (!isActive(occupier)) {
			refuse(
				INVALID_ARGUMENT,
				"INVALID_ARGUMENT",
				"invalid argument: topic_key: the record that owns \`" + topicKey + "\` is " + occupier.state,
			);
		}
		// One fact, one record: a save on the same title updates the record
		// and appends the state it replaced to the trail — revision N with
		// the content the record had, exactly as the engine does.
		config.history.push({
			memory_uuid: occupier.uuid,
			revision: occupier.revision,
			memory_type: occupier.memory_type,
			content: occupier.content,
			state: occupier.state,
			changed_by: "user",
			change_reason: "saved again with the topic key \`" + topicKey + "\`",
			changed_at: stamp(occupier.id),
		});
		occupier.revision += 1;
		occupier.content = content;
		occupier.title = title;
		occupier.keywords = keywords;
		occupier.updated_at = stamp(occupier.id);
		persist();
		process.stdout.write(
			JSON.stringify({
				uuid: occupier.uuid,
				row_id: occupier.id,
				title: occupier.title,
				keywords: occupier.keywords,
				topic_key: occupier.topic_key,
			}),
		);
	} else {
		const id = config.store.reduce((max, item) => Math.max(max, item.id ?? 0), 0) + 1;
		const item = {
			id,
			uuid: config.save.uuid,
			title,
			keywords,
			topic_key: topicKey,
			namespace,
			memory_type: "fact",
			content,
			state: "active",
			revision: 1,
			deleted: false,
			created_at: stamp(id),
			updated_at: stamp(id),
		};
		config.store.push(item);
		persist();
		process.stdout.write(
			JSON.stringify({
				uuid: item.uuid,
				row_id: item.id,
				title: item.title,
				keywords: item.keywords,
				topic_key: item.topic_key,
			}),
		);
	}
} else if (args[0] === "search") {
	const query = String(args[1] ?? "").toLowerCase();
	const terms = query.split(/\\s+/).filter(Boolean);
	const namespace = flag("--namespace");
	const limit = Math.min(Number(flag("--limit") ?? 20), 100);
	let matches = config.store.filter((item) => isActive(item));
	if (namespace && !config.leakForeign) {
		matches = matches.filter((item) => item.namespace === namespace);
	}
	matches = matches.filter((item) => {
		// Every word of the query counts (\`--match all\`), over the four
		// indexed columns.
		const tokens = indexedTokens(item);
		return terms.every((term) => tokens.has(term));
	});
	if (config.leakForeign) {
		matches = matches.filter((item) => item.state === "active");
	}
	// bm25 is not modelled: shorter content first, then id. The order only
	// decides what a saturated page cuts.
	matches = matches
		.sort((left, right) => left.content.length - right.content.length || byId(left, right))
		.slice(0, limit);
	process.stdout.write(
		JSON.stringify({
			query: args[1],
			namespace: namespace ?? null,
			count: matches.length,
			items: matches.map((item) => ({
				uuid: item.uuid,
				title: item.title,
				keywords: item.keywords,
				rank: -1,
				namespace: item.namespace,
				memory_type: item.memory_type,
				state: item.state,
				revision: item.revision,
				created_at: item.created_at,
				valid_from: null,
				valid_until: null,
			})),
			relations: [],
			read_scope: "restricted",
		}),
	);
} else if (args[0] === "list") {
	const namespace = flag("--namespace");
	const state = flag("--state");
	const value = flag("--value");
	const limit = Math.min(Number(flag("--limit") ?? 20), 100);
	const offset = Number(flag("--offset") ?? 0);
	const full = args.includes("--full");
	// An argument the engine cannot read is never an empty answer (S4): it
	// refuses the value with the message \`src/domain/value.rs\` writes.
	if (value !== undefined && !/^\\d{3,}$/.test(value.trim())) {
		refuse(
			INVALID_ARGUMENT,
			"INVALID_ARGUMENT",
			"invalid argument: value: '" +
				value.trim() +
				"' is not a value: a value is a run of 3 or more digits, exactly as written " +
				"(8081 is a value, 80 and 8a81 are not)",
		);
	}
	if (config.listFails) {
		refuse(1, "INTERNAL", "internal error: the store could not be read");
	}
	let items = config.store.filter((item) => item.deleted !== true);
	if (namespace && !config.leakForeign) {
		items = items.filter((item) => item.namespace === namespace);
	}
	if (state) {
		items = items.filter((item) => item.state === state);
	}
	// The value filter applies BEFORE the limit (S3): the answer is the
	// records that carry the value, and the offset pages THOSE. \`carriesValue\`
	// is the engine's rule (\`src/domain/value.rs\`).
	if (value !== undefined) {
		items = items.filter((item) => carriesValue(item, value.trim()));
	}
	// The engine orders by id and applies the limit AFTER the filters
	// (src/storage/memory_repo.rs:99-104): OLDEST first, then the offset.
	items = items.sort(byId).slice(offset, offset + limit);
	const shaped = items.map((item) =>
		full
			? item
			: {
					id: item.id,
					uuid: item.uuid,
					title: item.title,
					keywords: item.keywords,
					topic_key: item.topic_key,
					namespace: item.namespace,
					memory_type: item.memory_type,
					preview: item.content.slice(0, 80),
					state: item.state,
					revision: item.revision,
					created_at: item.created_at,
					updated_at: item.updated_at,
				},
	);
	process.stdout.write(
		JSON.stringify({ count: shaped.length, items: shaped, relations: [], read_scope: "restricted" }),
	);
} else if (args[0] === "show") {
	const item = config.store.find((entry) => entry.uuid === args[1]);
	if (!item) {
		refuse(NOT_FOUND, "NOT_FOUND", "not found: memory:" + args[1]);
	}
	process.stdout.write(JSON.stringify(item));
} else if (args[0] === "history") {
	const rows = config.history.filter((row) => row.memory_uuid === args[1]);
	// No \`memory_history\` row means no history: the engine answers count 0
	// and "no history for <uuid>", not an error (src/cli/commands.rs:1017).
	process.stdout.write(
		JSON.stringify({
			uuid: args[1],
			count: rows.length,
			items: rows.map((row) => ({
				revision: row.revision,
				memory_type: row.memory_type,
				state: row.state,
				content: row.content,
				changed_by: row.changed_by,
				change_reason: row.change_reason,
				changed_at: row.changed_at,
			})),
		}),
	);
} else if (args[0] === "supersede") {
	const olderUuid = flag("--older");
	const newerUuid = flag("--newer");
	const reason = flag("--reason");
	const older = config.store.find((entry) => entry.uuid === olderUuid);
	const newer = config.store.find((entry) => entry.uuid === newerUuid);
	if (!older) {
		refuse(NOT_FOUND, "NOT_FOUND", "not found: memory:" + olderUuid);
	}
	if (!newer) {
		refuse(NOT_FOUND, "NOT_FOUND", "not found: memory:" + newerUuid);
	}
	// A concurrent writer can win between the read the repair made
	// (\`list --value --full\`) and the write: the race the supersede-failure
	// test reproduces. Someone else retires the row first, so the engine
	// refuses the supersede because the record is no longer active.
	if ((config.supersedeRaces ?? []).includes(olderUuid) && older.state === "active") {
		older.state = "superseded";
		older.revision += 1;
		older.updated_at = stamp(older.id);
		persist();
	}
	if (older.namespace !== newer.namespace) {
		refuse(
			INVALID_ARGUMENT,
			"INVALID_ARGUMENT",
			"invalid argument: newer_uuid: supersede is same-namespace in MVP: " +
				olderUuid +
				" is in " +
				older.namespace +
				", " +
				newerUuid +
				" is in " +
				newer.namespace,
		);
	}
	if (older.state !== "active") {
		refuse(INVALID_ARGUMENT, "INVALID_ARGUMENT", "invalid argument: older: must be active, is " + older.state);
	}
	if (newer.state !== "active") {
		refuse(INVALID_ARGUMENT, "INVALID_ARGUMENT", "invalid argument: newer: must be active, is " + newer.state);
	}
	// Cycle rejection: an edge older -> newer closes a cycle when \`older\` is
	// already reachable from \`newer\` (src/storage/memory_repo.rs:650-670).
	const reachable = new Set([newerUuid]);
	let grew = true;
	while (grew) {
		grew = false;
		for (const link of config.links) {
			if (reachable.has(link.older) && !reachable.has(link.newer)) {
				reachable.add(link.newer);
				grew = true;
			}
		}
	}
	if (reachable.has(olderUuid)) {
		refuse(
			INVALID_ARGUMENT,
			"INVALID_ARGUMENT",
			"invalid argument: newer_uuid: supersede would close a cycle",
		);
	}
	// The flip is a change, so the trail keeps the state it replaces: the
	// revision before the bump, the content it had, and the link's reason
	// (src/storage/memory_repo.rs, \`supersede_atomic\`).
	config.history.push({
		memory_uuid: olderUuid,
		revision: older.revision,
		memory_type: older.memory_type,
		content: older.content,
		state: older.state,
		changed_by: "user",
		change_reason: reason,
		changed_at: stamp(older.id),
	});
	older.state = "superseded";
	older.revision += 1;
	older.updated_at = stamp(older.id);
	config.links.push({ older: olderUuid, newer: newerUuid, reason, at: older.updated_at });
	persist();
	process.stdout.write(JSON.stringify({ older: olderUuid, newer: newerUuid, ok: true }));
} else {
	process.stdout.write("{}");
}
`;

/**
 * The topic key a title yields, the way the engine derives it
 * (`src/domain/title.rs:26`): the same title always lands on the same record.
 */
function topicKeyOf(title) {
	let out = "";
	for (const char of String(title ?? "").trim()) {
		const lowered = char.toLowerCase();
		if (/^[\p{L}\p{N}]+$/u.test(lowered)) {
			out += lowered;
		} else if (out !== "" && !out.endsWith("-")) {
			out += "-";
		}
	}
	return out.replace(/^-+/, "").replace(/-+$/, "");
}

/** One record of the fake store, shaped the way `list --full` answers. */
function record({ id, uuid, title, content, keywords = [], state = "active", namespace = NAMESPACE }) {
	return {
		id,
		uuid,
		title,
		keywords,
		topic_key: topicKeyOf(title),
		namespace,
		memory_type: "fact",
		content,
		state,
		revision: 1,
		deleted: false,
		created_at: `2026-10-2${id % 9}T00:00:00Z`,
		updated_at: `2026-10-2${id % 9}T00:00:00Z`,
	};
}

/** The live record the corpus already holds, and the one a save creates. */
function twinStore(extra = []) {
	return [
		record({
			id: 1,
			uuid: "u-old",
			title: "Puerto de escucha del servicio",
			content: "El servicio payments-api escucha en el puerto 8081 (4 workers).",
			keywords: ["puerto", "port"],
		}),
		...extra,
	];
}

/** Filler records that carry no value the save could share. */
function fillerNotes(count) {
	const notes = [];
	for (let index = 0; index < count; index += 1) {
		notes.push(
			record({
				id: index + 2,
				uuid: `u-note-${index}`,
				title: `Nota de trabajo ${index}`,
				content: `Nota de trabajo sobre el despliegue y su operacion diaria.`,
				keywords: ["nota", "bitacora"],
			}),
		);
	}
	return notes;
}

/** Filler records that all carry the save's value, with another vocabulary. */
function fillerTickets(count, value) {
	const tickets = [];
	for (let index = 0; index < count; index += 1) {
		tickets.push(
			record({
				id: index + 2,
				uuid: `u-ticket-${index}`,
				title: `Ticket ${index}`,
				content: value,
				keywords: ["ticket"],
			}),
		);
	}
	return tickets;
}

/** A workspace, a store, and the hooks pi would hand to the extension. */
function scenario(
	t,
	{
		store = [],
		bound = true,
		writeNamespaces,
		leakForeign = false,
		candidate = null,
		listFails = false,
		supersedeRaces = [],
	} = {},
) {
	const dir = mkdtempSync(join(tmpdir(), "mnexis-recovery-"));
	t.after(() => rmSync(dir, { recursive: true, force: true }));
	const log = join(dir, "calls.log");
	const configPath = join(dir, "engine.json");
	writeFileSync(log, "");
	writeFileSync(
		configPath,
		JSON.stringify({
			namespace: NAMESPACE,
			bound,
			writeNamespaces: writeNamespaces ?? [NAMESPACE],
			store,
			history: [],
			links: [],
			save: { uuid: "u-new" },
			leakForeign,
			candidate,
			listFails,
			supersedeRaces,
		}),
	);
	const binary = join(dir, "mnexis");
	writeFileSync(binary, FAKE_BINARY);
	chmodSync(binary, 0o755);
	process.env.MNEXIS_FAKE_LOG = log;
	process.env.MNEXIS_FAKE_CONFIG = configPath;
	process.env.MNEXIS_BIN = binary;

	const tools = new Map();
	const entries = [];
	const pi = {
		registerTool(tool) {
			tools.set(tool.name, tool);
		},
		on() {},
		appendEntry(type, data) {
			entries.push({ type, data });
		},
	};
	mnexisMemory(pi);
	return { dir, log, configPath, tools, entries };
}

/** The context pi hands to a headless tool call. */
function context() {
	return {
		cwd: process.cwd(),
		hasUI: false,
		mode: "print",
		ui: { confirm: async () => false },
	};
}

/** Every argument list the fake engine received, in order. */
function calls(log) {
	return readFileSync(log, "utf8")
		.split("\n")
		.filter(Boolean)
		.map((line) => JSON.parse(line));
}

/** The whole state the fake engine kept. */
function engine(configPath) {
	return JSON.parse(readFileSync(configPath, "utf8"));
}

/** The store the fake engine kept. */
function storeOf(configPath) {
	return engine(configPath).store;
}

/** Run one tool the way pi does, and read everything it left behind. */
async function runTool(sc, name, params) {
	const tool = sc.tools.get(name);
	assert.ok(tool, `${name} is registered`);
	const result = await tool.execute("call-1", params, undefined, undefined, context());
	return {
		result,
		text: result.content.map((block) => block.text).join("\n"),
		details: result.details,
	};
}

/** Run one save through the tool, and read everything it left behind. */
async function save(sc, params) {
	const answer = await runTool(sc, "mnexis_save", params);
	const received = calls(sc.log);
	return {
		...answer,
		store: storeOf(sc.configPath),
		engine: engine(sc.configPath),
		supersedes: received.filter((args) => args[0] === "supersede"),
		lists: received.filter((args) => args[0] === "list"),
		searches: received.filter((args) => args[0] === "search"),
		entries: [...sc.entries],
	};
}

/** The live records the store holds, by uuid, in id order. */
function liveUuids(items) {
	return items
		.filter((item) => item.deleted !== true && item.state === "active")
		.sort((left, right) => left.id - right.id)
		.map((item) => item.uuid);
}

/** The live records that carry a value of three or more digits. */
function liveCarrying(items, value) {
	return items.filter(
		(item) =>
			item.deleted !== true &&
			item.state === "active" &&
			(String(item.content ?? "").match(/\d{3,}/g) ?? []).includes(value),
	);
}

/** The twin the save under a different title produces. */
const TWIN_SAVE = {
	title: "Puerto de la API de pagos",
	content: "El puerto del servicio es 8081.",
	keywords: "puerto, listen port",
};

test("a second save for one fact leaves exactly one live record (S1, S3, S4, S5)", async (t) => {
	// The measured corpus (R1): two live records, the same value, different
	// titles, one shared keyword. The save's record is the newer one.
	const sc = scenario(t, { store: twinStore() });
	const answer = await save(sc, TWIN_SAVE);

	assert.notEqual(answer.result.isError, true, "the save itself succeeded");
	assert.equal(answer.supersedes.length, 1, "exactly one repair");
	assert.deepEqual(
		[
			answer.supersedes[0][answer.supersedes[0].indexOf("--older") + 1],
			answer.supersedes[0][answer.supersedes[0].indexOf("--newer") + 1],
		],
		["u-old", "u-new"],
		"the older record is superseded onto the one the save created",
	);
	assert.match(answer.supersedes[0].join(" "), /8081/, "the reason names the shared value");

	assert.deepEqual(liveUuids(answer.store), ["u-new"], "one live record is left: the new one");
	assert.equal(liveCarrying(answer.store, "8081").length, 1, "one live record for the value");
	assert.equal(
		answer.store.find((item) => item.uuid === "u-old").state,
		"superseded",
		"the older record is kept, never deleted",
	);
	assert.equal(
		calls(sc.log).find((args) => args[0] === "delete" || args[0] === "purge"),
		undefined,
		"nothing is deleted or purged",
	);

	// S4: the model reads both records and the shared value.
	assert.match(answer.text, /Puerto de escucha del servicio/, "the older record is named");
	assert.match(answer.text, /Puerto de la API de pagos/, "the newer record is named");
	assert.match(answer.text, /8081/, "the shared value is named");
	assert.doesNotMatch(
		answer.text,
		/history/,
		"the one line does not narrate the trail; the trail is a store fact the model asks for",
	);

	// S4: one line per intervention on the extension's diagnostic path.
	const lines = answer.entries.filter((entry) => entry.type === "mnexis-store-recovery");
	assert.equal(lines.length, 1, "exactly one intervention line");
	assert.equal(lines[0].data.action, "supersede", "the line names the action");
	assert.equal(lines[0].data.value, "8081", "the line carries the shared value");
	assert.equal(lines[0].data.older.uuid, "u-old", "the line names the superseded record");
	assert.equal(lines[0].data.newer.uuid, "u-new", "the line names the surviving record");
});

test("every live twin is retired: three converge to one (B3, S1)", async (t) => {
	// Three records for one fact, all live, all carrying the same value and
	// sharing the word the fact is called by. S1 promises ONE live record;
	// retiring one of three leaves two.
	const sc = scenario(t, {
		store: [
			record({
				id: 1,
				uuid: "u-old-1",
				title: "Puerto de escucha del servicio",
				content: "El servicio escucha en el puerto 8081.",
				keywords: ["puerto"],
			}),
			record({
				id: 2,
				uuid: "u-old-2",
				title: "Puerto de la API",
				content: "El puerto de la API es 8081.",
				keywords: ["puerto", "port"],
			}),
			record({
				id: 3,
				uuid: "u-old-3",
				title: "Puerto del worker",
				content: "El worker usa el puerto 8081.",
				keywords: ["puerto"],
			}),
		],
	});
	const answer = await save(sc, TWIN_SAVE);

	assert.equal(answer.supersedes.length, 3, "every qualifying twin is retired");
	assert.deepEqual(
		answer.supersedes
			.map((args) => args[args.indexOf("--older") + 1])
			.sort(),
		["u-old-1", "u-old-2", "u-old-3"],
		"all three older records, not just the first one a page showed",
	);
	assert.deepEqual(liveUuids(answer.store), ["u-new"], "S1: exactly one live record is left");
	assert.equal(
		answer.store.filter((item) => item.state === "superseded").length,
		3,
		"and the three rows are kept",
	);

	// S4: every pair is named, and each intervention has its line.
	for (const uuid of ["u-old-1", "u-old-2", "u-old-3"]) {
		assert.match(answer.text, new RegExp(uuid), `the answer names ${uuid}`);
	}
	const lines = answer.entries.filter((entry) => entry.type === "mnexis-store-recovery");
	assert.equal(lines.length, 3, "one line per intervention");
	assert.deepEqual(lines.map((line) => line.data.older.uuid).sort(), ["u-old-1", "u-old-2", "u-old-3"]);
	assert.equal(answer.details.recovery.superseded.length, 3, "the details carry all three");
});

test("a twin is repaired in a namespace far larger than the scan bound (B1)", async (t) => {
	// The verifier's reproduction: a namespace of 141 live records, the fact
	// at the very first one. `list --limit N` answers the OLDEST rows
	// (src/storage/memory_repo.rs:99-104), so a repair that looks for the
	// record the save just wrote in a page of the namespace finds nothing
	// here and returns silently.
	const sc = scenario(t, { store: twinStore(fillerNotes(140)) });
	const answer = await save(sc, TWIN_SAVE);

	assert.notEqual(answer.result.isError, true, "the save itself succeeded");
	assert.equal(answer.supersedes.length, 1, "the twin was found and retired");
	assert.equal(liveCarrying(answer.store, "8081").length, 1, "one live record for the value");
	assert.deepEqual(liveCarrying(answer.store, "8081").map((item) => item.uuid), ["u-new"]);
	assert.equal(
		answer.store.find((item) => item.uuid === "u-old").state,
		"superseded",
		"the twin is history, not a second live record",
	);
	for (const note of answer.store.filter((item) => item.uuid.startsWith("u-note-"))) {
		assert.equal(note.state, "active", "the records around it are untouched");
	}
});

test("a twin behind a page of matches is still found (B1: the answer is exhaustive)", async (t) => {
	// 200 records that carry no value, 120 that carry it, and the long twin
	// AFTER them: the twin is the 121st match, so a repair that trusted one
	// page of matches (the engine's MAX_LIMIT, 100) would never see it, and a
	// repair that read the namespace instead of the matches would read 321
	// records in four pages. The answer to a filter is a set of matches, and
	// the repair reads it to the end (S6): no ranked page, no walk of the
	// namespace, no blind spot.
	const sc = scenario(t, {
		store: [
			...fillerNotes(200),
			...fillerTickets(120, "8081").map((item, index) => ({ ...item, id: index + 202 })),
			record({
				id: 322,
				uuid: "u-old",
				title: "Puerto de escucha del servicio",
				content: "El servicio payments-api escucha en el puerto 8081 (4 workers).",
				keywords: ["puerto", "port"],
			}),
		],
	});
	const answer = await save(sc, TWIN_SAVE);

	assert.notEqual(answer.result.isError, true, "the save itself succeeded");
	const twin = answer.store.find((item) => item.uuid === "u-old");
	const tickets = answer.store.filter((item) => item.uuid.startsWith("u-ticket-"));
	assert.equal(answer.store.filter((item) => item.uuid.startsWith("u-note-")).length, 200, "200 records carry no value");
	assert.equal(tickets.length, 120, "120 carry it");
	assert.equal(
		tickets.every((item) => item.id < twin.id),
		true,
		"the twin is the 121st match: one page of matches cannot reach it",
	);
	assert.equal(answer.supersedes.length, 1, "the twin behind the page was retired");
	// The tickets are a different fact that merely carries the same number;
	// the fact this save is about ends with one live record.
	assert.deepEqual(
		liveCarrying(answer.store, "8081")
			.filter((item) => !item.uuid.startsWith("u-ticket-"))
			.map((item) => item.uuid),
		["u-new"],
		"one live record for the fact",
	);
	assert.equal(
		tickets.every((item) => item.state === "active"),
		true,
		"the records that only share the value are untouched",
	);

	// The repair asks the engine for the records that carry the value — never
	// for a ranking — and pages the matches until a short page proves it has
	// seen them all. 121 matches are two pages, whatever the namespace holds,
	// and every page asks by value.
	assert.equal(answer.searches.length, 0, "no ranked query: a ranking is not a set");
	assert.equal(answer.lists.length, 2, "121 matches are two pages of matches, not four pages of records");
	assert.deepEqual(
		answer.lists.map((page) => [page[page.indexOf("--value") + 1], page[page.indexOf("--offset") + 1]]),
		[
			["8081", "0"],
			["8081", "100"],
		],
		"both pages ask by the value and step a whole page of matches",
	);
	for (const page of answer.lists) {
		assert.ok(page.includes("--full"), "each page carries the content the rule compares");
		assert.equal(page[page.indexOf("--namespace") + 1], NAMESPACE, "inside the save's namespace");
		assert.equal(page[page.indexOf("--limit") + 1], "100", "at the engine's own cap");
	}
});

test("the superseded row stays visible to list and show, and its trail keeps the state before the flip (S3, B4)", async (t) => {
	const sc = scenario(t, { store: twinStore() });
	await save(sc, TWIN_SAVE);

	// What the engine really does: the row is kept and reported `superseded`.
	// (`mnexis timeline` says the same thing with its `superseded by` link;
	// the extension exposes no timeline tool.)
	const shown = await runTool(sc, "mnexis_show", { uuid: "u-old" });
	assert.notEqual(shown.result.isError, true, "the superseded record is still readable");
	assert.equal(shown.details.memory.state, "superseded", "show reports the row as superseded");
	assert.match(shown.text, /superseded/, "and says so in the answer the model reads");
	assert.match(shown.text, /8081/, "and keeps what it said");

	const listed = await runTool(sc, "mnexis_list", { namespace: NAMESPACE });
	assert.notEqual(listed.result.isError, true);
	assert.match(listed.text, /"uuid":"u-old"/, "list still shows the row");
	assert.match(listed.text, /"state":"superseded"/, "list reports it as superseded");

	// The trail half: the supersede is a change, so the engine records the
	// state it replaced — revision 1, with the content the record had and
	// the link's reason — and `mnexis_history` answers it. It is the state
	// BEFORE the flip: the row is not reported as `superseded` there.
	const history = await runTool(sc, "mnexis_history", { uuid: "u-old" });
	assert.notEqual(history.result.isError, true, "history answers, it does not fail");
	assert.equal(history.details.count, 1, "the supersede wrote the replaced revision");
	assert.equal(history.details.items[0].revision, 1, "with the revision before the bump");
	assert.equal(
		history.details.items[0].content,
		"El servicio payments-api escucha en el puerto 8081 (4 workers).",
		"carrying the content the record had before the flip",
	);
	assert.equal(history.details.items[0].state, "active", "and the state it had");
	assert.match(history.text, /8081/, "the trail carries the old content");
	assert.doesNotMatch(history.text, /superseded/, "history is the pre-flip state, not the flip");
	assert.deepEqual(sc.entries.filter((entry) => entry.type === "revision"), [], "nothing else wrote a row");
});

test("fidelity: the fake records the state a change replaces, on both paths", async (t) => {
	// Guards the assertions above from being vacuous: the same fake writes a
	// revision when a save updates a record it already owns, and when a
	// supersede flips one. Both rows hold the state that was replaced, so
	// "count 1 after a supersede" is a fact about the engine, not about a
	// fake that invents history.
	const sc = scenario(t, { store: [] });
	const first = await save(sc, {
		title: "Puerto de escucha",
		content: "El puerto del servicio es 8081.",
		keywords: "puerto",
	});
	assert.equal(first.supersedes.length, 0, "the first save has no twin");
	const second = await save(sc, {
		title: "Puerto de escucha",
		content: "El puerto del servicio ahora es 8081.",
		keywords: "puerto",
	});
	assert.equal(second.supersedes.length, 0, "a save on its own record is not a twin of itself");
	const history = await runTool(sc, "mnexis_history", { uuid: "u-new" });
	assert.equal(history.details.count, 1, "the update wrote the replaced revision");
	assert.equal(history.details.items[0].revision, 1, "with the pre-update revision number");
	assert.equal(
		history.details.items[0].content,
		"El puerto del servicio es 8081.",
		"and the content the record had before the update",
	);
	assert.deepEqual(liveUuids(second.store), ["u-new"], "and the record is one, not two");
});

test("a repair that cannot read the store journals the failure and changes nothing (S4)", async (t) => {
	const sc = scenario(t, { store: twinStore(), listFails: true });
	const answer = await save(sc, TWIN_SAVE);

	assert.notEqual(answer.result.isError, true, "the save itself is untouched by a failed repair");
	assert.doesNotMatch(answer.text, /Repaired a duplicate/, "the answer claims no repair");
	assert.equal(liveUuids(answer.store).length, 2, "the store is exactly what the engine left");
	const lines = answer.entries.filter((entry) => entry.type === "mnexis-store-recovery");
	assert.equal(lines.length, 1, "the failure is not silent");
	assert.equal(lines[0].data.action, "failed", "the line says it failed");
	assert.equal(lines[0].data.step, "list", "the step that could not read the store");
	assert.equal(lines[0].data.value, "8081", "naming the value it asked the engine about");
	assert.match(lines[0].data.error, /store could not be read/, "with what the engine said");
	assert.equal(lines[0].data.saved.uuid, "u-new", "and which save it could not check");
	assert.equal(answer.details.recovery_failed[0].step, "list", "the tool result carries it too");
	// A repair that could not run is declared to the model, never hidden behind
	// a clean-looking save: the caller must be able to see that a second live
	// record may remain and check it.
	assert.match(
		answer.text,
		/The duplicate check did not finish/,
		"the answer declares the unfinished check to the model",
	);
	assert.match(answer.text, /may remain/, "and says what that means for the store");
	assert.match(answer.text, /store could not be read/, "naming the engine's own reason");
});

test("a supersede the engine refuses is journaled, never reported as done (S4)", async (t) => {
	// A concurrent writer retires the twin between the read and the write, so
	// the engine answers "must be active, is superseded". The extension must
	// report the failed intervention instead of claiming the repair.
	const sc = scenario(t, { store: twinStore(), supersedeRaces: ["u-old"] });
	const answer = await save(sc, TWIN_SAVE);

	assert.notEqual(answer.result.isError, true, "the save itself is untouched");
	assert.equal(answer.supersedes.length, 1, "the write was attempted");
	assert.doesNotMatch(answer.text, /Repaired a duplicate/, "the answer claims no repair");
	const lines = answer.entries.filter((entry) => entry.type === "mnexis-store-recovery");
	assert.equal(lines.length, 1, "one line for the failed intervention");
	assert.equal(lines[0].data.action, "failed");
	assert.equal(lines[0].data.step, "supersede");
	assert.equal(lines[0].data.uuid, "u-old", "it names the record it could not retire");
	assert.match(lines[0].data.error, /must be active, is superseded/, "with the engine's own words");
	assert.equal(answer.details.recovery, undefined, "and no repair is reported in the details");
});

test("the candidate query asks by the value, bounded to the engine's own cap", async (t) => {
	const sc = scenario(t, { store: twinStore() });
	const answer = await save(sc, TWIN_SAVE);

	assert.equal(answer.searches.length, 0, "no ranked read: a ranking is not a set, and it cannot be paged");
	assert.equal(answer.lists.length, 1, "one page of matches is the whole answer");
	assert.deepEqual(
		answer.lists[0],
		[
			"list",
			"--namespace",
			NAMESPACE,
			"--limit",
			"100",
			"--offset",
			"0",
			"--full",
			"--value",
			"8081",
			"--json",
		],
		"the question is the value the twin must carry, at the engine's MAX_LIMIT and from the first match",
	);

	// A save with no value of three digits cannot produce a twin (S2.2), so
	// the store is not read at all.
	const quiet = scenario(t, { store: twinStore() });
	const withoutValue = await save(quiet, {
		title: "Sin numeros",
		content: "esta memoria no lleva ningun valor",
		keywords: "puerto, port",
	});
	assert.equal(withoutValue.searches.length, 0, "no value, no ranked read");
	assert.equal(withoutValue.lists.length, 0, "and the store is not read at all");
	assert.equal(withoutValue.supersedes.length, 0, "and no repair");
});

test("a save with nothing to repair answers exactly what it always did", async (t) => {
	// PRESERVE: the shipped answer of a save, untouched by the repair.
	const sc = scenario(t, { store: twinStore() });
	const answer = await save(sc, {
		title: "Sin numeros",
		content: "esta memoria no lleva ningun valor",
		keywords: "puerto, port",
	});
	assert.equal(
		answer.text,
		"Saved a memory in namespace 'payments'. Words: puerto, port.",
		"the answer is the one the engine and the extension always produced",
	);
	assert.equal(answer.details.recovery, undefined, "no repair is reported");
	assert.equal(answer.details.recovery_failed, undefined, "and nothing failed");
	assert.equal(answer.entries.length, 0, "and nothing is journaled");
});

test("a save that lands on its own record still retires the other live twin", async (t) => {
	// S1 does not only cover a save that inserts: a save whose title lands on
	// a record the store already holds updates that record, and the fact can
	// still be live twice. The updated record is the newer write, so the other
	// live record is the one that becomes history.
	const sc = scenario(t, {
		store: [
			record({
				id: 1,
				uuid: "u-old",
				title: "Puerto de escucha del servicio",
				content: "El servicio payments-api escucha en el puerto 8081 (4 workers).",
				keywords: ["puerto", "port"],
			}),
			record({
				id: 2,
				uuid: "u-other",
				title: "Puerto de la API de pagos",
				content: "El puerto del servicio es 8081.",
				keywords: ["puerto", "listen port"],
			}),
		],
	});
	const answer = await save(sc, {
		title: "Puerto de escucha del servicio",
		content: "El servicio escucha en el puerto 8081.",
		keywords: "puerto, port",
	});

	assert.notEqual(answer.result.isError, true, "the save succeeded");
	assert.equal(answer.details.uuid, "u-old", "the save landed on the record the title owns");
	assert.equal(answer.supersedes.length, 1, "the other live twin is retired");
	assert.deepEqual(
		[
			answer.supersedes[0][answer.supersedes[0].indexOf("--older") + 1],
			answer.supersedes[0][answer.supersedes[0].indexOf("--newer") + 1],
		],
		["u-other", "u-old"],
		"onto the record the save updated, which is the newer write",
	);
	assert.deepEqual(liveUuids(answer.store), ["u-old"], "the fact ends with one live record");
	assert.equal(
		answer.store.find((item) => item.uuid === "u-other").state,
		"superseded",
		"the other record is history",
	);
	// The update writes the revision it replaced for u-old; the supersede
	// writes one for the OTHER record (u-other), not for u-old.
	const history = await runTool(sc, "mnexis_history", { uuid: "u-old" });
	assert.equal(history.details.count, 1, "one revision row for the update");
	assert.equal(history.details.items[0].revision, 1, "the revision the update replaced");
});

test("a changed value is a conflict, not a twin: nothing is superseded", async (t) => {
	// Control: the same store and the same save with the same value is the
	// twin the feature must repair, so the boundary below is a real difference.
	const control = await save(scenario(t, { store: twinStore() }), TWIN_SAVE);
	assert.equal(control.supersedes.length, 1, "the same value under another title is a twin");

	// The measured conflict (cnf): the live record says 8081, the save 9090.
	const sc = scenario(t, { store: twinStore() });
	const answer = await save(sc, {
		title: "Puerto de la API de pagos",
		content: "El puerto del servicio ahora es 9090.",
		keywords: "puerto, listen port",
	});
	assert.notEqual(answer.result.isError, true, "the save itself is untouched");
	assert.equal(answer.supersedes.length, 0, "a changed value is never repaired");
	assert.equal(answer.entries.length, 0, "and nothing is reported as an intervention");
	assert.equal(
		answer.store.find((item) => item.uuid === "u-old").state,
		"active",
		"the live record is untouched",
	);
	assert.equal(
		answer.store.find((item) => item.uuid === "u-new").state,
		"active",
		"the new record is untouched too",
	);
	assert.doesNotMatch(answer.text, /Repaired a duplicate/, "the answer claims no repair");
});

test("a shared number with no shared keyword is not touched", async (t) => {
	// Control: the same number and one shared keyword is a twin.
	const control = await save(scenario(t, { store: twinStore() }), TWIN_SAVE);
	assert.equal(control.supersedes.length, 1, "a shared keyword is what makes it one fact");

	// The measured false positive (L2): 8081 as a port and 8081 as a worker
	// count are two facts, and the vocabulary says so.
	const sc = scenario(t, { store: twinStore() });
	const answer = await save(sc, {
		title: "Trabajadores del servicio",
		content: "El servicio corre con 8081 workers.",
		keywords: "workers, concurrency",
	});
	assert.equal(answer.supersedes.length, 0, "a shared number alone is not a twin");
	assert.equal(answer.entries.length, 0, "and nothing is reported as an intervention");
	assert.deepEqual(
		answer.store.filter((item) => item.state === "active").map((item) => item.uuid).sort(),
		["u-new", "u-old"].sort(),
		"both facts are still live",
	);
});

test("only active records can be a twin", async (t) => {
	// Control: the active record with the same value and keyword is repaired.
	const control = await save(scenario(t, { store: twinStore() }), TWIN_SAVE);
	assert.equal(control.supersedes.length, 1, "an active record is a twin");

	// Boundary: the corpus already holds the same fact as a superseded row.
	// It is history, not a second live record, and pairing it would rewrite
	// history instead of the store.
	const sc = scenario(t, {
		store: [
			record({
				id: 2,
				uuid: "u-history",
				title: "Puerto de escucha del servicio",
				content: "El servicio payments-api escucha en el puerto 8081.",
				keywords: ["puerto", "port"],
				state: "superseded",
			}),
		],
	});
	const answer = await save(sc, TWIN_SAVE);
	assert.equal(answer.supersedes.length, 0, "a superseded row is not a live twin");
	assert.equal(answer.entries.length, 0, "and nothing is reported as an intervention");
	assert.equal(
		answer.store.find((item) => item.uuid === "u-history").state,
		"superseded",
		"the history is untouched",
	);
	assert.equal(
		answer.store.find((item) => item.uuid === "u-new").state,
		"active",
		"the save stands on its own",
	);
});

test("a record in another namespace is never a twin", async (t) => {
	// Control: the same record in the save's namespace is a twin.
	const control = await save(scenario(t, { store: twinStore() }), TWIN_SAVE);
	assert.equal(control.supersedes.length, 1, "the same namespace is a twin");

	// Boundary: the engine may answer wider than the filter asked. The
	// extension checks the namespace itself, so a foreign record is never
	// superseded (S2.1, S3).
	const sc = scenario(t, {
		store: [
			record({
				id: 1,
				uuid: "u-foreign",
				title: "Puerto de escucha del servicio",
				content: "El servicio payments-api escucha en el puerto 8081 (4 workers).",
				keywords: ["puerto", "port"],
				namespace: OTHER_NAMESPACE,
			}),
		],
		leakForeign: true,
	});
	const answer = await save(sc, TWIN_SAVE);
	assert.equal(answer.supersedes.length, 0, "a record outside the namespace is never repaired");
	assert.equal(answer.entries.length, 0, "and nothing is reported as an intervention");
	assert.equal(
		answer.store.find((item) => item.uuid === "u-foreign").state,
		"active",
		"the other project's record is untouched",
	);
});

test("a compaction summary is never a twin", async (t) => {
	// Control: the same fact as an ordinary record is a twin.
	const control = await save(scenario(t, { store: twinStore() }), TWIN_SAVE);
	assert.equal(control.supersedes.length, 1, "an ordinary record is a twin");

	// Boundary 1: the record already in the store is the summary the
	// shipped session_compact hook writes. It is session history, not a
	// project fact.
	const summary = "Session abc was compacted. What was happening: el puerto 8081.";
	const existingSummary = await save(
		scenario(t, {
			store: [
				record({
					id: 1,
					uuid: "u-summary",
					title: "",
					content: summary,
					keywords: ["puerto"],
				}),
			],
		}),
		TWIN_SAVE,
	);
	assert.equal(existingSummary.supersedes.length, 0, "a summary is not a live fact");
	assert.equal(existingSummary.entries.length, 0, "and nothing is reported as an intervention");
	assert.equal(
		existingSummary.store.find((item) => item.uuid === "u-summary").state,
		"active",
		"the summary is untouched",
	);

	// Boundary 2: neither record may be a summary — a save that writes one
	// is not a fact either.
	const savedSummary = await save(scenario(t, { store: twinStore() }), {
		title: "Session xyz was compacted",
		content: "Session xyz was compacted. What was happening: el puerto 8081.",
		keywords: "puerto, port",
	});
	assert.equal(savedSummary.supersedes.length, 0, "a save of a summary is not a twin");
	assert.equal(savedSummary.entries.length, 0, "and nothing is reported as an intervention");
	assert.equal(
		savedSummary.store.find((item) => item.uuid === "u-old").state,
		"active",
		"the live fact is untouched",
	);
});

test("an unbound workspace repairs nothing", async (t) => {
	// Control: bound and writable, the twin is repaired.
	const control = await save(scenario(t, { store: twinStore() }), TWIN_SAVE);
	assert.equal(control.supersedes.length, 1, "a bound workspace repairs the twin");

	// Boundary: no binding means no namespace is a permission (S3), so the
	// save never lands and there is nothing to repair.
	const sc = scenario(t, { store: twinStore(), bound: false, candidate: NAMESPACE });
	const answer = await save(sc, TWIN_SAVE);
	assert.equal(answer.result.isError, true, "the write is refused");
	assert.equal(answer.supersedes.length, 0, "an unbound workspace repairs nothing");
	assert.equal(answer.entries.length, 0, "and writes no intervention line");
	assert.equal(
		answer.store.find((item) => item.uuid === "u-old").state,
		"active",
		"the store is untouched",
	);
});

test("a namespace the profile does not allow for writes repairs nothing", async (t) => {
	// Control: the allowed namespace is repaired.
	const control = await save(scenario(t, { store: twinStore() }), TWIN_SAVE);
	assert.equal(control.supersedes.length, 1, "an allowed namespace repairs the twin");

	// Boundary: bound, but the write allowlist does not include it (S3).
	const sc = scenario(t, { store: twinStore(), writeNamespaces: [] });
	const answer = await save(sc, TWIN_SAVE);
	assert.equal(answer.result.isError, true, "the write is refused");
	assert.equal(answer.supersedes.length, 0, "a namespace without write permission is not repaired");
	assert.equal(answer.entries.length, 0, "and writes no intervention line");
	assert.equal(
		answer.store.find((item) => item.uuid === "u-old").state,
		"active",
		"the store is untouched",
	);
});

test("a save that introduces a different number is not a repair: the value did not change", async (t) => {
	// The boundary S2.4 needed and the shared-token rule could not express: a
	// content that carries the old value AND a new one shares the old token
	// with the live record, but the two contents are not one signature, so the
	// save is a correction and the store must be left alone.
	const control = await save(scenario(t, { store: twinStore() }), TWIN_SAVE);
	assert.equal(control.supersedes.length, 1, "the identical signature under another title is a twin");

	const sc = scenario(t, { store: twinStore() });
	const answer = await save(sc, {
		title: "Puerto de la API de pagos",
		content: "El puerto del servicio es 8081, y el ticket 1234 lo confirma.",
		keywords: "puerto, listen port",
	});
	assert.notEqual(answer.result.isError, true, "the save itself is untouched");
	assert.equal(answer.supersedes.length, 0, "a second number is not a rename");
	assert.equal(answer.entries.length, 0, "and nothing is reported as an intervention");
	const live = answer.store.filter((item) => item.state === "active");
	assert.equal(live.length, 2, "the live records are exactly what the engine left");
});
