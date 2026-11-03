/**
 * The decisions, without a host and without the binary.
 *
 * Every one of these is a rule the plugin applies on both plugin models, so
 * a mistake here is a mistake in two hosts at once. The pins at the end read
 * the repository's own files: the plugin is one flat file, so its copy of the
 * vocabulary and of the engine's tool ids has to be checked against the engine
 * — the only contract every host shares. Nothing here reads another host's
 * extension tree.
 */

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import {
	BUSY_EXIT_CODE,
	BUSY_RETRY_DELAYS_MS,
	CURATED_DESCRIPTIONS,
	GRANT_REFUSAL,
	PROFILE_REFUSAL,
	READ_TIMEOUT_MS,
	STATE_CHARS,
	SUMMARY_CHARS,
	TYPE_GLOSSES,
	WRITE_TIMEOUT_MS,
	createChildRegistry,
	createClient,
	createDeadline,
	gateViolation,
	isHumanMessage,
	parseJson,
	pointerText,
	promptTextFromParts,
	sessionIDOfEvent,
	stateBlockText,
} from "../index.js";

const STATUS = {
	workspace: { key: "git:example.com-acme", namespace: "acme", candidate_namespace: null },
	memory_count: 42,
};

const PREVIOUS = {
	session: {
		id: "sess-previous",
		started_at: "2026-10-30T22:04:11Z",
		summary: "Wired the budget into the door.\nThe second line is not carried.",
	},
};

test("a shell call that grants a namespace is refused with the command the human must run", () => {
	for (const tool of ["bash", "shell"]) {
		assert.equal(
			gateViolation(tool, { command: "mnexis config allow-namespace acme" }),
			GRANT_REFUSAL,
			tool,
		);
		assert.equal(
			gateViolation(tool, { command: "cd /repo && mnexis  project   bind acme --key k --path ." }),
			GRANT_REFUSAL,
			tool,
		);
	}
	assert.match(GRANT_REFUSAL, /mnexis config allow-namespace/);
	assert.match(GRANT_REFUSAL, /nothing was granted/);
});

test("a shell call that does not grant anything is the model's to make", () => {
	for (const command of [
		"mnexis context 'the port we agreed on'",
		"mnexis config show",
		"mnexis project list",
		"git status",
		"",
	]) {
		assert.equal(gateViolation("bash", { command }), null, command);
	}
	// A tool that carries no command at all is not a grant.
	assert.equal(gateViolation("bash", {}), null);
	assert.equal(gateViolation("bash", undefined), null);
});

test("an edit of the mnexis profile is refused, and nothing else is", () => {
	for (const tool of ["edit", "write", "patch"]) {
		assert.equal(
			gateViolation(tool, { filePath: "/home/u/.mnexis/mnexis.toml" }),
			PROFILE_REFUSAL,
			tool,
		);
		assert.equal(
			gateViolation(tool, { filePath: "~/.mnexis/projects.toml" }),
			PROFILE_REFUSAL,
			tool,
		);
	}
	assert.match(PROFILE_REFUSAL, /managed by the mnexis CLI/);
	assert.equal(gateViolation("write", { filePath: "/repo/notes.toml" }), null);
	assert.equal(gateViolation("write", { filePath: "/repo/src/mnexis.toml.rs" }), null);
	assert.equal(gateViolation("write", { path: "/repo/docs/projects.toml" }), PROFILE_REFUSAL);
	// Reading the profile is not editing it.
	assert.equal(gateViolation("read", { filePath: "/home/u/.mnexis/mnexis.toml" }), null);
	assert.equal(gateViolation("task", { filePath: "/home/u/.mnexis/mnexis.toml" }), null);
});

test("the text of a prompt is its own words, never the host's", () => {
	assert.equal(
		promptTextFromParts([
			{ type: "text", text: "haz la extension de opencode de calidad" },
			{ type: "file", mime: "image/png", url: "data:image/png;base64,AA" },
		]),
		"haz la extension de opencode de calidad",
	);
	assert.equal(
		promptTextFromParts([
			{ type: "text", text: "what the human said" },
			{ type: "text", text: "auto-continue", synthetic: true },
		]),
		"what the human said",
	);
	assert.equal(promptTextFromParts([{ type: "text", text: "only the host", synthetic: true }]), "");
	assert.equal(promptTextFromParts([]), "");
	assert.equal(promptTextFromParts(undefined), "");
	assert.equal(promptTextFromParts([{ type: "text" }, { type: "text", text: "   " }]), "");
});

test("a message the host sent is not a message the human typed", () => {
	assert.equal(isHumanMessage({ role: "user" }), true);
	assert.equal(isHumanMessage({}), true);
	assert.equal(isHumanMessage({ role: "assistant" }), false);
	assert.equal(isHumanMessage(null), true);
});

test("the session id an event names is read in both hosts' shapes", () => {
	// opencode 1.x
	assert.equal(
		sessionIDOfEvent({ type: "session.created", properties: { info: { id: "oc-1", directory: "/repo" } } }),
		"oc-1",
	);
	// opencode 2.x
	assert.equal(sessionIDOfEvent({ type: "session.created", data: { sessionID: "oc-2" } }), "oc-2");
	assert.equal(sessionIDOfEvent({ type: "session.idle", properties: { sessionID: "oc-3" } }), "oc-3");
	assert.equal(sessionIDOfEvent({ type: "session.created" }), null);
	assert.equal(sessionIDOfEvent(null), null);
});

test("the compaction block is the engine's live state, bounded, and says what did not fit", () => {
	const data = {
		count: 2,
		items: [
			{
				uuid: "8b4f9c2e-1111-4a1a-9f1f-1e2d3c4b5a69",
				title: "The door answers previews",
				memory_type: "decision",
				revision: 3,
				content: "memory_context answers a 160-character preview per item; the whole record is memory_get.",
			},
			{ uuid: "u2", title: "", memory_type: "project_state", state: "active", content: "" },
		],
	};
	const block = stateBlockText(data);
	assert.match(block, /^mnexis — the live state of this project/);
	assert.match(block, /- \[decision\] The door answers previews \(8b4f9c2e-1111-4a1a-9f1f-1e2d3c4b5a69, rev 3\)/);
	assert.match(block, /160-character preview per item/);
	// A record with no content carries nothing: an empty line in the block
	// would be a fact the model reads as "there is nothing more".
	assert.doesNotMatch(block, /\(u2/);

	assert.equal(stateBlockText({ items: [] }), "");
	assert.equal(stateBlockText({ items: [{ uuid: "u", content: "   " }] }), "");
	assert.equal(stateBlockText(null), "");
});

test("the compaction block stops at its budget and counts what it left out", () => {
	const items = [];
	for (let i = 0; i < 6; i += 1) {
		items.push({
			uuid: `u${i}`,
			title: `record ${i}`,
			memory_type: "decision",
			content: "x".repeat(Math.floor(STATE_CHARS / 4)),
		});
	}
	const block = stateBlockText({ items });
	const carried = (block.match(/^- \[/gm) ?? []).length;
	assert.ok(carried >= 1 && carried < 6, `carried ${carried}`);
	assert.match(block, new RegExp(`… ${6 - carried} more live record`));
});

test("the pointer names the namespace, the door and the previous session", () => {
	const text = pointerText({ status: STATUS, previous: PREVIOUS });
	assert.match(text, /namespace `acme` \(42 memories\)/);
	assert.match(text, /mnexis_memory_context/);
	// The tool has a second name under opencode 2's Code Mode, and a dead
	// reference would be worse than a longer line.
	assert.match(text, /tools\.mnexis\.memory_context/);
	assert.match(text, /Previous session \(2026-10-30\): Wired the budget into the door\./);
	assert.doesNotMatch(text, /second line/, "the pointer carries one line, not the whole summary");
	assert.match(text, /never copy the conversation into `memory_save_prompt`/);
});

test("the pointer says nothing when there is no namespace to name", () => {
	assert.equal(pointerText({ status: { workspace: { namespace: null } } }), "");
	assert.equal(pointerText({ status: {} }), "");
	assert.equal(pointerText({}), "");
	assert.equal(pointerText(), "");
	// A count the engine did not answer is not "0 memories".
	assert.match(pointerText({ status: { workspace: { namespace: "acme" } } }), /namespace `acme`\./);
});

test("the pointer bounds the previous summary it carries", () => {
	const long = "y".repeat(SUMMARY_CHARS * 2);
	const text = pointerText({ status: STATUS, previous: { session: { started_at: "", summary: long } } });
	const line = text.split("\n").find((l) => l.startsWith("Previous session"));
	assert.ok(line.includes("date unknown"));
	assert.ok(line.length <= SUMMARY_CHARS + 40, `line of ${line.length}`);
	assert.ok(line.endsWith("…"));
	// No summary recorded is not an error and draws nothing extra.
	const without = pointerText({ status: STATUS, previous: { session: { summary: null } } });
	assert.equal(without.split("\n").some((l) => l.startsWith("Previous session")), false);
});

test("the CLI's JSON is read even when something else wrote around it", () => {
	assert.deepEqual(parseJson('{"a":1}'), { a: 1 });
	assert.deepEqual(parseJson('\nwarning: x\n{"a":1}\n'), { a: 1 });
	assert.equal(parseJson(""), null);
	assert.equal(parseJson("not json"), null);
});

// --- the engine pins (odd/tasks/opencode-parity.md, S1+S2) -----------------
//
// The curated definitions and the seven glosses are an inline copy in the one
// flat file the plugin must stay, because the plugin cannot import from the
// engine. A copy that may drift is a copy nobody can trust, so each pin reads
// the engine's own source, from the repository root. The engine is the only
// contract the hosts share; no pin reads another host's extension tree.

const REPO_ROOT = fileURLToPath(new URL("../../..", import.meta.url));

/** A repository file, read from disk — the copy is pinned to the file. */
function repoFile(relative) {
	return readFileSync(join(REPO_ROOT, relative), "utf8");
}

/**
 * The engine's seven type names, read from its own source.
 *
 * `SaveArgs::memory_type`'s doc comment states the closed set ("Memory type:
 * knowledge, decision, preference, procedure, project_state, episode or
 * fact."), and schemars carries that doc into the MCP input schema every host
 * reads. The extraction drops the `///` markers, so a reflowed comment is
 * still the same list.
 */
function engineTypeNames() {
	const source = repoFile("src/mcp/tools.rs");
	const match = source.match(/Memory type:\s*([^.]*)\./);
	assert.ok(match, "the engine's save argument states the memory types");
	const text = match[1]
		.split("\n")
		.map((line) => line.replace(/^\s*\/\/+\s*/, "").trim())
		.join(" ");
	return text
		.split(/\s*(?:,|\bor\b)\s*/)
		.map((name) => name.trim())
		.filter((name) => name !== "");
}

test("the plugin's seven type names are the engine's closed set, and the gloss is the plugin's own", () => {
	const engine = engineTypeNames();
	assert.equal(engine.length, 7, `the engine states seven kinds: ${engine.join(", ")}`);
	assert.equal(new Set(engine).size, 7, "the engine's names are distinct");

	const plugin = TYPE_GLOSSES.map((gloss) => {
		const [name, ...rest] = String(gloss).split(" — ");
		assert.notEqual(rest.join(" — ").trim(), "", `a gloss is a name and one line: ${gloss}`);
		assert.doesNotMatch(rest.join(" — "), /\n/, `a gloss is one line: ${gloss}`);
		return name;
	});

	assert.deepEqual(
		plugin,
		engine,
		"the plugin's names are the engine's, in the engine's order: the engine is the shared contract",
	);
});

test("the curated map carries exactly one non-empty description per MCP tool id", () => {
	// The engine's tool inventory is the id list (src/mcp/server.rs). The host
	// namespaces an MCP tool `<server>_<tool>`, so `memory_save` is listed as
	// `mnexis_memory_save`.
	const ids = [...repoFile("src/mcp/server.rs").matchAll(/\[tool\(\s*name = "([a-z_]+)"/g)].map(
		(match) => `mnexis_${match[1]}`,
	);
	assert.equal(ids.length, 17, `the engine publishes its tool inventory: ${ids}`);

	assert.deepEqual(
		Object.keys(CURATED_DESCRIPTIONS).sort(),
		[...ids].sort(),
		"one curated entry per engine tool, and no id the engine does not publish",
	);
	for (const [id, text] of Object.entries(CURATED_DESCRIPTIONS)) {
		assert.equal(typeof text, "string", id);
		assert.notEqual(text.trim(), "", `the description of ${id} is not empty`);
	}
});

// --- the reliability patterns (odd/tasks/opencode-parity.md, S3) -----------
//
// The host awaits these calls, so each of them gets its own ceiling, and a
// store that is momentarily locked is worth one more try on the commands whose
// identity the engine owns. Both patterns are the engine-compatible ones the
// pi extension measured; the plugin keeps its own copy.

/** Fake exec: records calls and replays scripted answers. */
function scriptedExec(answers = []) {
	const calls = [];
	const exec = async (command, args, options) => {
		calls.push({ command, args, options });
		return answers.shift() ?? { code: 0, stdout: "{}", stderr: "" };
	};
	return { exec, calls };
}

const BUSY = { code: BUSY_EXIT_CODE, stdout: "", stderr: "busy: sql: database is locked" };

test("every CLI call carries a budget: a read ten seconds, a write thirty", async () => {
	const { exec, calls } = scriptedExec();
	const client = createClient({ exec });

	await client.status("/repo");
	await client.previousSession("/repo");
	await client.stateRecords("/repo");
	await client.sessionStart({ sessionId: "s1" }, "/repo");
	await client.sessionEnd({ sessionId: "s1" }, "/repo");
	await client.savePrompt({ content: "hola", sessionId: "s1" }, "/repo");

	assert.deepEqual(
		calls.map((call) => call.options.timeout),
		[READ_TIMEOUT_MS, READ_TIMEOUT_MS, READ_TIMEOUT_MS, WRITE_TIMEOUT_MS, WRITE_TIMEOUT_MS, WRITE_TIMEOUT_MS],
	);
});

test("a call bound to a smaller budget passes exactly that to the child", async () => {
	const { exec, calls } = scriptedExec();
	const client = createClient({ exec });

	await client.withBudget(700).savePrompt({ content: "hola" }, "/repo");

	assert.equal(calls[0].options.timeout, 700);
});

test("a locked store is tried again on a call that is idempotent by construction", async () => {
	const { exec, calls } = scriptedExec([BUSY, { code: 0, stdout: '{"id":"s1"}', stderr: "" }]);
	const client = createClient({ exec });

	const result = await client.sessionStart({ sessionId: "s1" }, "/repo");

	assert.equal(calls.length, 2, "the start was tried twice");
	assert.equal(result.code, 0, "the answer that came back is the one kept");
	assert.equal(result.data.id, "s1");
	assert.equal(result.busy_retries, 1, "the answer says how many times it was tried again");
});

test("a locked store is tried at most twice more, and then reports the lock", async () => {
	const { exec, calls } = scriptedExec([BUSY, BUSY, BUSY, BUSY, BUSY]);
	const client = createClient({ exec });

	const result = await client.savePrompt({ content: "hola", sessionId: "s1" }, "/repo");

	assert.equal(calls.length, 1 + BUSY_RETRY_DELAYS_MS.length, "the tries are bounded");
	assert.equal(result.code, BUSY_EXIT_CODE, "a store that stays locked is still a failure");
	assert.equal(result.busy_retries, BUSY_RETRY_DELAYS_MS.length);
});

test("a locked read is not tried again: an answer that never came is one report", async () => {
	for (const call of [
		(client) => client.status("/repo"),
		(client) => client.previousSession("/repo"),
		(client) => client.stateRecords("/repo"),
	]) {
		const { exec, calls } = scriptedExec([BUSY, { code: 0, stdout: "{}", stderr: "" }]);
		const result = await call(createClient({ exec }));

		assert.equal(calls.length, 1);
		assert.equal(result.code, BUSY_EXIT_CODE);
		assert.equal(result.busy_retries, 0);
	}
});

test("a step that runs out of budget is abandoned, never awaited forever", async () => {
	const deadline = createDeadline(20, "prompt capture");

	const answer = await deadline.step("capture", () => new Promise(() => {}));

	assert.match(answer.abandoned, /capture did not finish within the .* prompt capture budget/);
});

test("a step that fits keeps its answer, and a spent budget abandons without work", async () => {
	const deadline = createDeadline(500, "pointer");
	const answer = await deadline.step("status", async () => "ok");
	assert.equal(answer.value, "ok");

	const spent = createDeadline(0, "pointer");
	let ran = false;
	const skipped = await spent.step("status", async () => {
		ran = true;
		return "never";
	});
	assert.match(skipped.abandoned, /already spent/);
	assert.equal(ran, false, "work with no budget left is never started");
});

test("the child registry kills what it tracks, once, and forgets what exited", () => {
	const registry = createChildRegistry();
	const killed = [];
	const exited = [];
	const first = {
		kill: (signal) => killed.push(signal),
		once: (name, callback) => exited.push({ name, callback }),
	};
	registry.track(first);
	registry.track({ kill: () => killed.push("SIGTERM"), once: () => {} });

	// A child that exits on its own is no longer the registry's to kill.
	exited.find((entry) => entry.name === "exit").callback();

	registry.killAll();
	assert.deepEqual(killed, ["SIGTERM"]);
	registry.killAll();
	assert.deepEqual(killed, ["SIGTERM"], "the registry is emptied by the first kill");
});
