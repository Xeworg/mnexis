/**
 * The tools the extension registers, driven the way pi drives them.
 *
 * A fake `mnexis` binary records the arguments it receives, so the tests
 * pin what the extension asks the CLI for (not what the CLI answers).
 */

import { test } from "node:test";
import assert from "node:assert/strict";
import { chmodSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import mnexisMemory from "../index.js";

/** A fake `mnexis` that records its arguments and answers envelopes. */
const FAKE_BINARY = `#!/usr/bin/env node
import { appendFileSync, readFileSync } from "node:fs";
const log = process.env.MNEXIS_FAKE_LOG;
const args = process.argv.slice(2);
appendFileSync(log, JSON.stringify(args) + "\\n");
const calls = readFileSync(log, "utf8")
	.split("\\n")
	.filter(Boolean)
	.map((line) => JSON.parse(line));
// The namespace the extension bound this workspace to, learned from the
// bind call it made.
const bind = calls.filter((call) => call[0] === "project" && call[1] === "bind").pop();
const workspace = bind ? bind[2] : "mnexis";
if (args[0] === "project" && args[1] === "list") {
	process.stdout.write(JSON.stringify({ bindings: [] }));
} else if (args[0] === "config" && args[1] === "show") {
	process.stdout.write(
		JSON.stringify({ scope: { read_namespaces: [workspace], write_namespaces: [workspace] } }),
	);
} else if (args[0] === "show") {
	process.stdout.write(
		JSON.stringify({
			uuid: args[1],
			namespace: process.env.MNEXIS_FAKE_MEMORY_NAMESPACE || workspace,
		}),
	);
} else if (args[0] === "search" || args[0] === "list") {
	process.stdout.write(JSON.stringify({ count: 0, items: [] }));
} else {
	process.stdout.write("{}");
}
`;

/** Install the fake binary and point the extension at it. */
function setup() {
	const dir = mkdtempSync(join(tmpdir(), "mnexis-tools-"));
	const log = join(dir, "calls.log");
	writeFileSync(log, "");
	const binary = join(dir, "mnexis");
	writeFileSync(binary, FAKE_BINARY);
	chmodSync(binary, 0o755);
	process.env.MNEXIS_FAKE_LOG = log;
	process.env.MNEXIS_BIN = binary;
	return { dir, log };
}

/** A host that only records the registered tools. */
function fakeHost() {
	const tools = new Map();
	return {
		tools,
		pi: {
			registerTool(tool) {
				tools.set(tool.name, tool);
			},
			on() {},
		},
	};
}

/** Every argument list the fake binary received, in order. */
function calls(log) {
	return readFileSync(log, "utf8")
		.split("\n")
		.filter(Boolean)
		.map((line) => JSON.parse(line));
}

/** The context pi hands to a tool. */
function context() {
	return {
		cwd: process.cwd(),
		hasUI: false,
		mode: "print",
		ui: { confirm: async () => false },
	};
}

test("search reads across projects unless a namespace is given", async () => {
	const { log } = setup();
	const { tools, pi } = fakeHost();
	mnexisMemory(pi);
	const search = tools.get("mnexis_search");
	assert.ok(search, "mnexis_search is registered");

	const result = await search.execute("t1", { query: "shared" }, undefined, undefined, context());
	assert.notEqual(result.isError, true);

	const searchCall = calls(log).find((args) => args[0] === "search");
	assert.ok(searchCall, "search was called");
	assert.equal(
		searchCall.includes("--namespace"),
		false,
		`search must not narrow to one project: ${JSON.stringify(searchCall)}`,
	);
});

test("an explicit namespace still narrows the search", async () => {
	const { log } = setup();
	const { tools, pi } = fakeHost();
	mnexisMemory(pi);
	const search = tools.get("mnexis_search");

	await search.execute(
		"t2",
		{ query: "shared", namespace: "otro-proyecto" },
		undefined,
		undefined,
		context(),
	);

	const searchCall = calls(log).find((args) => args[0] === "search");
	assert.ok(searchCall, "search was called");
	assert.deepEqual(searchCall, [
		"search",
		"shared",
		"--namespace",
		"otro-proyecto",
		"--json",
	]);
});

test("list reads across projects unless a namespace is given", async () => {
	const { log } = setup();
	const { tools, pi } = fakeHost();
	mnexisMemory(pi);
	const list = tools.get("mnexis_list");
	assert.ok(list, "mnexis_list is registered");

	const result = await list.execute("t3", {}, undefined, undefined, context());
	assert.notEqual(result.isError, true);

	const listCall = calls(log).find((args) => args[0] === "list");
	assert.ok(listCall, "list was called");
	assert.equal(
		listCall.includes("--namespace"),
		false,
		`list must not narrow to one project: ${JSON.stringify(listCall)}`,
	);
});

test("a save targets the workspace namespace", async () => {	const { log } = setup();
	const { tools, pi } = fakeHost();
	mnexisMemory(pi);
	const save = tools.get("mnexis_save");
	assert.ok(save, "mnexis_save is registered");

	const result = await save.execute(
		"t4",
		{ content: "a fact" },
		undefined,
		undefined,
		context(),
	);
	assert.notEqual(result.isError, true, "the workspace namespace is authorized");

	const saveCall = calls(log).find((args) => args[0] === "save");
	assert.ok(saveCall, "save was called");
	assert.equal(saveCall.includes("--namespace"), true, "save names its namespace");
});

test("a supersede inside the workspace namespace goes through", async () => {
	const { log } = setup();
	const { tools, pi } = fakeHost();
	mnexisMemory(pi);
	const supersede = tools.get("mnexis_supersede");
	assert.ok(supersede, "mnexis_supersede is registered");

	const result = await supersede.execute(
		"t5",
		{ older: "old-uuid", newer: "new-uuid", reason: "corrected" },
		undefined,
		undefined,
		context(),
	);
	assert.notEqual(result.isError, true, "the supersede is allowed");

	const call = calls(log).find((args) => args[0] === "supersede");
	assert.ok(call, "supersede was called");
});

test("a marker names the namespace the extension binds", async () => {
	const { log, dir } = setup();
	writeFileSync(join(dir, ".mnexis-namespace"), "ns-marcado\n");
	const { tools, pi } = fakeHost();
	mnexisMemory(pi);
	const save = tools.get("mnexis_save");
	assert.ok(save, "mnexis_save is registered");

	await save.execute("t7", { content: "a fact" }, undefined, undefined, {
		...context(),
		cwd: dir,
	});

	const bind = calls(log).find((args) => args[0] === "project" && args[1] === "bind");
	assert.ok(bind, "the workspace was bound");
	assert.equal(bind[2], "ns-marcado", "the marker names the namespace");
});

test("a supersede of another project's memory is refused", async () => {
	const { log } = setup();
	process.env.MNEXIS_FAKE_MEMORY_NAMESPACE = "otro-proyecto";
	const { tools, pi } = fakeHost();
	mnexisMemory(pi);
	const supersede = tools.get("mnexis_supersede");
	assert.ok(supersede, "mnexis_supersede is registered");

	const result = await supersede.execute(
		"t6",
		{ older: "old-uuid", newer: "new-uuid", reason: "corrected" },
		undefined,
		undefined,
		context(),
	);
	assert.equal(result.isError, true, "the supersede is refused");
	const text = result.content.map((block) => block.text).join(" ");
	assert.match(text, /otro-proyecto/, "the message names the other namespace");
	assert.match(text, /writes only into/, "the message states the boundary");

	const call = calls(log).find((args) => args[0] === "supersede");
	assert.equal(call, undefined, "no supersede reached the CLI");
	delete process.env.MNEXIS_FAKE_MEMORY_NAMESPACE;
});
