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
import { appendFileSync } from "node:fs";
const args = process.argv.slice(2);
appendFileSync(process.env.MNEXIS_FAKE_LOG, JSON.stringify(args) + "\\n");
if (args[0] === "project" && args[1] === "list") {
	process.stdout.write(JSON.stringify({ bindings: [] }));
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

test("a save without consent never reaches the CLI", async () => {
	const { log } = setup();
	const { tools, pi } = fakeHost();
	mnexisMemory(pi);
	const save = tools.get("mnexis_save");
	assert.ok(save, "mnexis_save is registered");

	// The write path needs the profile: the fake answers `config show`
	// with an empty scope, so the gate refuses and nothing is written.
	const result = await save.execute(
		"t4",
		{ content: "a fact" },
		undefined,
		undefined,
		context(),
	);
	assert.equal(result.isError, true, "the save is refused without consent");

	const saveCall = calls(log).find((args) => args[0] === "save");
	assert.equal(saveCall, undefined, "no save reached the CLI");
});
