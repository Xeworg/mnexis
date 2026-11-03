/**
 * The loadout the extension declares to the model — T3 of
 * odd/tasks/ext-ts-rewrite.md: B as amended by AB1/AB7, C, D, A as amended by
 * AB4, and AB5's move of the taxonomy into `mnexis_save`.
 *
 * A loadout is only a decision if the registration carries it, so every test
 * here reads the definitions the extension hands pi, never the source file.
 * The loadout is B's falsifiable half: the five door tools stay declared, the
 * long tail is `deferred` (reachable through `tool_search` and bullet 4), the
 * two lifecycle tools are registered inactive because the extension runs the
 * lifecycle itself, and the prompt writer is `hidden` because the input hook
 * already captures every message (AB7).
 *
 * AB1 rides here too: a deferred tail without an active `tool_search` is seven
 * dead ends, so the session_start handler activates it when — and only when —
 * the host offers a `tool_search` that declares `reachesTools`, the way pi's
 * own MCP extension does.
 */

import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import mnexisMemory from "../index.ts";

/**
 * The 14 tools, with the exposure and activation B decides (AB7 amends B).
 *
 * `inactive` is the registration's own `defaultActive: false`: the tool stays
 * registered and reachable by name, but it is never declared to the model.
 */
const LOADOUT = [
	// The door: declared and active, unchanged.
	{ name: "mnexis_workspace", exposure: "direct", inactive: false },
	{ name: "mnexis_context", exposure: "direct", inactive: false },
	{ name: "mnexis_search", exposure: "direct", inactive: false },
	{ name: "mnexis_show", exposure: "direct", inactive: false },
	{ name: "mnexis_save", exposure: "direct", inactive: false },
	// The tail: `tool_search` reaches it, nothing declares it per request.
	{ name: "mnexis_history", exposure: "deferred", inactive: false },
	{ name: "mnexis_list", exposure: "deferred", inactive: false },
	{ name: "mnexis_supersede", exposure: "deferred", inactive: false },
	{ name: "mnexis_suggest_topic_key", exposure: "deferred", inactive: false },
	{ name: "mnexis_recall", exposure: "deferred", inactive: false },
	{ name: "mnexis_search_prompts", exposure: "deferred", inactive: false },
	// AB7: the input hook already captures every prompt, so the tool is not
	// even searchable in pi (the MCP surface is another host's).
	{ name: "mnexis_save_prompt", exposure: "hidden", inactive: false },
	// The lifecycle: registered, never declared — the extension runs it.
	{ name: "mnexis_session_start", exposure: "direct", inactive: true },
	{ name: "mnexis_session_end", exposure: "direct", inactive: true },
];

/** The nine read tools: a permission extension can tell them from writes (D). */
const READ_TOOLS = [
	"mnexis_workspace",
	"mnexis_context",
	"mnexis_search",
	"mnexis_show",
	"mnexis_list",
	"mnexis_history",
	"mnexis_suggest_topic_key",
	"mnexis_recall",
	"mnexis_search_prompts",
];

/** The four tools whose calls mutate the store or the profile (C). */
const SEQUENTIAL_TOOLS = [
	"mnexis_save",
	"mnexis_supersede",
	"mnexis_session_start",
	"mnexis_session_end",
];

/** The four guideline bullets, one line each (A as amended by AB4). */
const GUIDELINES = {
	start:
		"At the start of a session in an existing project, call mnexis_context once with " +
		"a one-line task summary before doing anything else.",
	read:
		"Before changing code that already exists, call mnexis_search to find what is " +
		"already recorded about it, then mnexis_show the hit you need.",
	save:
		"Before reporting the task done, record a durable decision, discovery or user " +
		"preference with mnexis_save.",
	reach:
		"The mnexis engine has more tools than these; use tool_search to find and enable " +
		"the one a task needs.",
};

/** The record types a save states one line each, as the settle nudge states them. */
const TYPE_GLOSSES = [
	"knowledge — a durable fact about the domain, the tools or the data.",
	"decision — a choice made, and what it beat.",
	"preference — how the user wants things done.",
	"procedure — the steps that reliably achieve something.",
	"project_state — where the work stands now.",
	"episode — something that happened in a session.",
	"fact — the neutral default when none of the others fit.",
];

/**
 * A host that records the tool definitions it was given.
 *
 * `pi.getAllTools()` is not given: the registration tests never ask the host
 * for the tool set, and a host that cannot answer must not be a host that
 * breaks a session.
 */
function registered() {
	const tools = new Map();
	mnexisMemory({
		registerTool(tool) {
			tools.set(tool.name, tool);
		},
		on() {},
		appendEntry() {},
	});
	return tools;
}

/**
 * A host with the tool-set API pi has, for the AB1 activation tests.
 *
 * `toolSearch` stands for the built-in tool pi registers inactive: the test
 * decides whether the host offers one, and whether it declares `reachesTools`.
 */
function dynamicHost(t, { active = ["read"], toolSearch = null } = {}) {
	const dir = mkdtempSync(join(tmpdir(), "mnexis-loadout-"));
	t.after(() => rmSync(dir, { recursive: true, force: true }));
	const handlers = new Map();
	const activeSet = [...active];
	const calls = [];
	mnexisMemory({
		registerTool(tool) {
			if ((tool.exposure ?? "direct") === "direct" && tool.defaultActive !== false) {
				activeSet.push(tool.name);
			}
		},
		on(event, handler) {
			const list = handlers.get(event) ?? [];
			list.push(handler);
			handlers.set(event, list);
		},
		appendEntry() {},
		getAllTools() {
			return [...(toolSearch ? [toolSearch] : [])];
		},
		getActiveTools() {
			return [...activeSet];
		},
		setActiveTools(names) {
			calls.push([...names]);
			activeSet.splice(0, activeSet.length, ...names);
		},
	});
	return { handlers, activeSet, calls, dir };
}

/** Fire one event through the handlers the extension registered for it. */
async function emit(handlers, event, payload, ctx) {
	const results = [];
	for (const handler of handlers.get(event) ?? []) {
		results.push(await handler({ type: event, ...payload }, ctx));
	}
	return results;
}

/** The context of a session the engine never has to hear about. */
function context(cwd) {
	return {
		cwd,
		hasUI: false,
		mode: "print",
		ui: { confirm: async () => false },
	};
}

test("the loadout declares the door, defers the tail, and keeps the lifecycle inactive", () => {
	const tools = registered();

	assert.deepEqual(
		[...tools.keys()].sort(),
		LOADOUT.map((row) => row.name).sort(),
		"the 14 tools the loadout decides",
	);
	for (const row of LOADOUT) {
		const tool = tools.get(row.name);
		assert.ok(tool, `${row.name} is registered`);
		assert.equal(
			tool.exposure ?? "direct",
			row.exposure,
			`${row.name} reaches the model as '${row.exposure}'`,
		);
		assert.equal(
			tool.defaultActive === false,
			row.inactive,
			`${row.name} ${row.inactive ? "is" : "is not"} registered inactive`,
		);
	}
});

test("the mutating tools run one at a time (C)", () => {
	const tools = registered();

	assert.deepEqual(
		[...tools.values()]
			.filter((tool) => tool.executionMode === "sequential")
			.map((tool) => tool.name)
			.sort(),
		[...SEQUENTIAL_TOOLS].sort(),
		"exactly the mutating tools are serialized: parallel writes contend on the store's lock",
	);
});

test("the nine read tools declare readOnlyHint, and the writes declare nothing false (D)", () => {
	const tools = registered();

	assert.deepEqual(
		[...tools.values()]
			.filter((tool) => tool.annotations?.readOnlyHint === true)
			.map((tool) => tool.name)
			.sort(),
		[...READ_TOOLS].sort(),
		"the read-only hint names the tools that never write",
	);
	for (const tool of tools.values()) {
		if (READ_TOOLS.includes(tool.name)) {
			continue;
		}
		assert.notEqual(
			tool.annotations?.readOnlyHint,
			true,
			`${tool.name} writes, so it must not claim to be read-only`,
		);
	}
});

test("the start verb is mnexis_context's guideline, and no description claims FIRST (A, AB4)", () => {
	const tools = registered();
	const expected = {
		mnexis_context: [GUIDELINES.start, GUIDELINES.reach],
		mnexis_search: [GUIDELINES.read],
		mnexis_show: [GUIDELINES.read],
		mnexis_save: [GUIDELINES.save],
	};
	for (const [name, guidelines] of Object.entries(expected)) {
		assert.deepEqual(
			tools.get(name)?.promptGuidelines,
			guidelines,
			`${name} carries its bullet${guidelines.length === 1 ? "" : "s"}`,
		);
	}
	assert.deepEqual(
		[...tools.values()]
			.filter((tool) => Array.isArray(tool.promptGuidelines) && tool.promptGuidelines.length > 0)
			.map((tool) => tool.name)
			.sort(),
		Object.keys(expected).sort(),
		"only the declared tools carry guidelines: a deferred tool's bullets would name dead ends",
	);

	// AB4: two competing FIRSTs become one start verb, owned by the guideline.
	assert.doesNotMatch(
		tools.get("mnexis_workspace").description,
		/\bFIRST\b/,
		`the workspace no longer competes for the first call: ${tools.get("mnexis_workspace").description}`,
	);
	assert.match(
		GUIDELINES.start,
		/mnexis_context/,
		"the start bullet names the tool it commands",
	);

	// Each bullet names its tool, and bullet 4 teaches the verb instead of
	// naming the hidden names it would have to invent a UUID for.
	assert.match(GUIDELINES.read, /mnexis_search/);
	assert.match(GUIDELINES.read, /mnexis_show/);
	assert.match(GUIDELINES.save, /mnexis_save/);
	assert.match(GUIDELINES.reach, /tool_search/);
	for (const hidden of ["mnexis_list", "mnexis_history", "mnexis_supersede", "mnexis_recall"]) {
		assert.doesNotMatch(
			GUIDELINES.reach,
			new RegExp(hidden),
			`the reach bullet names the verb, never a deferred name (${hidden})`,
		);
	}
});

test("the save description carries the taxonomy the nudge used to teach (AB5)", () => {
	const tools = registered();
	const description = tools.get("mnexis_save").description;

	// The examples: the judgment the model needs at the moment it decides.
	assert.match(
		description,
		/What deserves to survive, for example:/,
		`the examples are labelled: ${description}`,
	);
	for (const example of [
		"a decision an external change forced",
		"a discovery about how this repo actually works",
		"a preference the user stated",
	]) {
		assert.ok(
			description.includes(example),
			`the save names the example "${example}": ${description}`,
		);
	}

	// The seven types, one line each, exactly as the opencode plugin states
	// them: one vocabulary, two consumers.
	const lines = description.split("\n");
	for (const gloss of TYPE_GLOSSES) {
		assert.equal(
			lines.filter((line) => line === gloss).length,
			1,
			`exactly one line states "${gloss}": ${description}`,
		);
	}
	assert.equal(
		lines.filter((line) => /^(knowledge|decision|preference|procedure|project_state|episode|fact) — /.test(line))
			.length,
		TYPE_GLOSSES.length,
		"and no type line the plugin does not carry",
	);
});

test("tool_search is activated at session_start when the host's tool reaches tools (AB1)", async (t) => {
	// pi registers `tool_search` inactive and declares `reachesTools` on the
	// one that can load a deferred tool; nothing activates it for another
	// extension, so the extension does it itself.
	const { handlers, activeSet, calls, dir } = dynamicHost(t, {
		active: ["read"],
		toolSearch: { name: "tool_search", reachesTools: () => "all" },
	});
	const before = [...activeSet];

	await emit(handlers, "session_start", { reason: "new" }, context(dir));

	assert.deepEqual(
		calls,
		[[...before, "tool_search"]],
		"the deferred tail is reachable from the first turn: one activation, no duplicate",
	);
	assert.ok(activeSet.includes("tool_search"));
});

test("a host without tool_search, or without the tool-set API, is left alone (AB1)", async (t) => {
	// A host whose `tool_search` is another extension's tool of that name and
	// declares no `reachesTools` is not a host that can reach the tail.
	const foreign = dynamicHost(t, {
		toolSearch: { name: "tool_search", reachesTools: undefined },
	});
	await emit(foreign.handlers, "session_start", { reason: "new" }, context(foreign.dir));
	assert.deepEqual(foreign.calls, [], "a tool_search that reaches nothing is never activated");

	// The same session on a host that offers no tool at all: no crash, no
	// call, and the session still works.
	const bare = dynamicHost(t);
	await emit(bare.handlers, "session_start", { reason: "new" }, context(bare.dir));
	assert.deepEqual(bare.calls, [], "a host with no tool_search is not a host to throw on");
});
