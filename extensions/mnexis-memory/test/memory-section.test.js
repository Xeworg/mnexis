/**
 * The conditional `<mnexis_memory>` section (odd/tasks/ext-ts-rewrite.md, AB3).
 *
 * `promptGuidelines` is the always-on baseline; the second half is one prompt
 * section that only exists where it can be true — the workspace is bound AND
 * its namespace holds memories. The decision is made once, at
 * `session_start`, from one bounded engine call, and cached: the handler that
 * runs on every run never touches the engine, and it never returns
 * `systemPrompt` (which would bust the prompt cache); it mutates
 * `event.systemPromptOptions.sections` in place, so pi can diff the sections
 * and append only what changed.
 */

import { test } from "node:test";
import assert from "node:assert/strict";
import { chmodSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import mnexisMemory from "../index.ts";

/** A fake `mnexis` whose binding and memory count the test decides. */
const FAKE_BINARY = `#!/usr/bin/env node
import { appendFileSync } from "node:fs";
const args = process.argv.slice(2);
appendFileSync(process.env.MNEXIS_FAKE_LOG, JSON.stringify(args) + "\\n");
const out = (value) => process.stdout.write(JSON.stringify(value));
if (args[0] === "project" && args[1] === "list") {
	const bound = process.env.MNEXIS_FAKE_BOUND !== "0";
	out({
		bindings: bound ? [{ key: "git:github.com-xeworg-mnexis", namespace: "mnexis" }] : [],
		current_key: "git:github.com-xeworg-mnexis",
	});
} else if (args[0] === "list") {
	if (process.env.MNEXIS_FAKE_LIST_FAIL) {
		process.stderr.write(process.env.MNEXIS_FAKE_LIST_FAIL);
		process.exit(1);
	}
	const count = Number(process.env.MNEXIS_FAKE_MEMORIES ?? "0");
	out({
		count,
		items: Array.from({ length: count }, (_, index) => ({
			uuid: "m" + index,
			namespace: "mnexis",
			title: "t" + index,
		})),
	});
} else if (args[0] === "session" || args[0] === "end") {
	out({ id: "01JX", namespace: "mnexis" });
} else if (args[0] === "status") {
	out({
		workspace: {
			key: "git:github.com-xeworg-mnexis",
			namespace: "mnexis",
			candidate_namespace: null,
			candidate_refusal: null,
		},
		scope: { default_namespace: "default", read_namespaces: ["mnexis"], write_namespaces: ["mnexis"] },
		memory_count: Number(process.env.MNEXIS_FAKE_MEMORIES ?? "0"),
		recent: [],
	});
} else if (args[0] === "config" && args[1] === "show") {
	out({ scope: { read_namespaces: ["mnexis"], write_namespaces: ["mnexis"] } });
} else {
	out({});
}
`;

/** Install the fake binary, and the binding and count this test decides. */
function setup(t, { bound = true, memories = 3 } = {}) {
	const dir = mkdtempSync(join(tmpdir(), "mnexis-section-"));
	t.after(() => rmSync(dir, { recursive: true, force: true }));
	const log = join(dir, "calls.log");
	writeFileSync(log, "");
	const binary = join(dir, "mnexis");
	writeFileSync(binary, FAKE_BINARY);
	chmodSync(binary, 0o755);
	const previous = {
		bin: process.env.MNEXIS_BIN,
		log: process.env.MNEXIS_FAKE_LOG,
		bound: process.env.MNEXIS_FAKE_BOUND,
		memories: process.env.MNEXIS_FAKE_MEMORIES,
	};
	process.env.MNEXIS_BIN = binary;
	process.env.MNEXIS_FAKE_LOG = log;
	process.env.MNEXIS_FAKE_BOUND = bound ? "1" : "0";
	process.env.MNEXIS_FAKE_MEMORIES = String(memories);
	t.after(() => {
		for (const [key, value] of [
			["MNEXIS_BIN", previous.bin],
			["MNEXIS_FAKE_LOG", previous.log],
			["MNEXIS_FAKE_BOUND", previous.bound],
			["MNEXIS_FAKE_MEMORIES", previous.memories],
		]) {
			if (value === undefined) {
				delete process.env[key];
			} else {
				process.env[key] = value;
			}
		}
	});
	return { dir, log };
}

/** A host that records its handlers. */
function fakeHost() {
	const handlers = new Map();
	mnexisMemory({
		registerTool() {},
		on(event, handler) {
			const list = handlers.get(event) ?? [];
			list.push(handler);
			handlers.set(event, list);
		},
		appendEntry() {},
	});
	return { handlers };
}

/** The context of a headless session with an id the engine can know. */
function context(cwd = process.cwd()) {
	return {
		cwd,
		hasUI: false,
		mode: "print",
		ui: { confirm: async () => false },
		sessionManager: { getSessionId: () => "01JX" },
	};
}

/** Fire one event through the handlers the extension registered for it. */
async function emit(handlers, event, payload, ctx) {
	const results = [];
	for (const handler of handlers.get(event) ?? []) {
		results.push(await handler({ type: event, ...payload }, ctx));
	}
	return results;
}

/** The prompt event pi hands `before_agent_start`, with its mutable options. */
function promptEvent(sections = {}) {
	return { messageType: "user", prompt: "hola", systemPromptOptions: { sections } };
}

/** The calls the fake binary received. */
function calls(log) {
	return readFileSync(log, "utf8")
		.split("\n")
		.filter(Boolean)
		.map((line) => JSON.parse(line));
}

test("a bound workspace with memories adds one section naming the namespace", async (t) => {
	setup(t, { bound: true, memories: 3 });
	const { handlers } = fakeHost();
	const ctx = context();
	await emit(handlers, "session_start", { reason: "new" }, ctx);

	const event = promptEvent();
	const results = await emit(handlers, "before_agent_start", event, ctx);

	assert.deepEqual(results, [undefined], `the handler never returns a systemPrompt: ${JSON.stringify(results)}`);
	const sections = event.systemPromptOptions.sections;
	assert.deepEqual(Object.keys(sections), ["mnexis-memory"], `one section was added: ${JSON.stringify(sections)}`);
	const section = sections["mnexis-memory"];
	assert.match(section, /'mnexis'/, `the namespace is named: ${section}`);
	assert.match(section, /3 memories/, `roughly how many records it holds: ${section}`);
	assert.match(section, /decisions and discoveries/i, `what lives there: ${section}`);
	assert.match(section, /mnexis_context/, `the start verb: ${section}`);
	assert.match(section, /mnexis_search/, `the read before changing code: ${section}`);
	assert.equal(
		section.split("\n").length,
		1,
		`one short paragraph, not a block: ${JSON.stringify(section)}`,
	);
});

test("the section is added once, however many runs follow", async (t) => {
	setup(t, { bound: true, memories: 2 });
	const { handlers } = fakeHost();
	const ctx = context();
	await emit(handlers, "session_start", { reason: "new" }, ctx);

	const sections = {};
	for (const run of ["first", "second", "third"]) {
		const event = promptEvent(sections);
		await emit(handlers, "before_agent_start", event, ctx);
		assert.deepEqual(Object.keys(sections), ["mnexis-memory"], `${run}: still one section`);
	}
	assert.equal(
		sections["mnexis-memory"].split("mnexis_context").length - 1,
		1,
		`the paragraph is not appended twice: ${sections["mnexis-memory"]}`,
	);
});

test("an unbound workspace adds nothing", async (t) => {
	setup(t, { bound: false, memories: 3 });
	const { handlers } = fakeHost();
	const ctx = context();
	await emit(handlers, "session_start", { reason: "new" }, ctx);

	const event = promptEvent();
	const results = await emit(handlers, "before_agent_start", event, ctx);

	assert.deepEqual(results, [undefined], "the handler says nothing");
	assert.deepEqual(event.systemPromptOptions.sections, {}, "no section is added");
});

test("a bound namespace with no memories adds nothing", async (t) => {
	setup(t, { bound: true, memories: 0 });
	const { handlers } = fakeHost();
	const ctx = context();
	await emit(handlers, "session_start", { reason: "new" }, ctx);

	const event = promptEvent();
	const results = await emit(handlers, "before_agent_start", event, ctx);

	assert.deepEqual(results, [undefined], "the handler says nothing");
	assert.deepEqual(event.systemPromptOptions.sections, {}, "an empty namespace has nothing to point at");
});

test("the handler never calls the engine: the decision is cached at session_start", async (t) => {
	const { log } = setup(t, { bound: true, memories: 4 });
	const { handlers } = fakeHost();
	const ctx = context();
	await emit(handlers, "session_start", { reason: "new" }, ctx);

	// Exactly the one cheap count the cache needs, in the bound namespace.
	const probe = calls(log).filter((args) => args[0] === "list");
	assert.equal(probe.length, 1, `one bounded count at the session's start: ${JSON.stringify(probe)}`);
	assert.equal(
		probe[0][probe[0].indexOf("--namespace") + 1],
		"mnexis",
		`the count is the workspace's own namespace: ${JSON.stringify(probe[0])}`,
	);

	const before = calls(log).length;
	for (let run = 0; run < 3; run += 1) {
		await emit(handlers, "before_agent_start", promptEvent(), ctx);
	}
	assert.equal(calls(log).length, before, "the prompt handler made no engine call");
});

test("other sections are kept, and the new one rides beside them", async (t) => {
	setup(t, { bound: true, memories: 1 });
	const { handlers } = fakeHost();
	const ctx = context();
	await emit(handlers, "session_start", { reason: "new" }, ctx);

	const sections = { rules: "a pi section the human's prompt owns" };
	await emit(handlers, "before_agent_start", promptEvent(sections), ctx);

	assert.equal(sections.rules, "a pi section the human's prompt owns", "the host's own section is untouched");
	assert.ok(sections["mnexis-memory"], "the memory section rides beside it");
});

test("a probe that cannot answer journals why and never blocks the session", (t) => {
	setup(t, { bound: true, memories: 4 });
	// A store that cannot be read is not an empty one: the section stays
	// out, and the failure reaches the journal instead of the turn.
	process.env.MNEXIS_FAKE_LIST_FAIL = "the store is locked";
	t.after(() => {
		delete process.env.MNEXIS_FAKE_LIST_FAIL;
	});
	const entries = [];
	const handlers = new Map();
	mnexisMemory({
		registerTool() {},
		on(event, handler) {
			const list = handlers.get(event) ?? [];
			list.push(handler);
			handlers.set(event, list);
		},
		appendEntry(customType, data) {
			entries.push({ customType, data });
		},
	});
	const ctx = context();

	return (async () => {
		await emit(handlers, "session_start", { reason: "new" }, ctx);
		const event = promptEvent();
		await emit(handlers, "before_agent_start", event, ctx);

		assert.deepEqual(event.systemPromptOptions.sections, {}, "nothing is claimed about a store nobody read");
		const probe = entries.find((entry) => entry.data?.step === "memory-probe");
		assert.ok(probe, `the failure was journaled: ${JSON.stringify(entries)}`);
		assert.equal(
			probe.customType,
			"mnexis-store-recovery",
			"it rides the journal kind that reports engine trouble",
		);
		assert.match(probe.data.error, /locked/, `the reason is carried: ${JSON.stringify(probe.data)}`);
	})();
});
