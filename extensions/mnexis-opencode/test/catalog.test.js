/**
 * The model-facing catalog (opencode 2 only).
 *
 * `ctx.tool.transform` is the one seat that rewrites an MCP tool's definition
 * before a model reads it, so it is the only place the plugin's own words reach a
 * model as a tool description. The map is pinned to the engine's inventory in
 * `engine-pins.test.js`; here it is driven through the host's editor.
 */

import assert from "node:assert/strict";
import test from "node:test";

import { CURATED_DESCRIPTIONS, TYPE_GLOSSES, createPlugin } from "../index.js";
import { engine, fakeHost, recordingExec } from "./helpers/host.js";

// --- the model-facing catalog (V2 only) -----------------------------------

test("v2: the catalog seat sits beside the tool gate", async () => {
	const { exec } = recordingExec(engine());
	const host = fakeHost();
	const cleanup = await createPlugin({ exec, env: {} }).setup(host.ctx);

	assert.equal(host.transforms.length, 1);
	assert.ok(host.hooks.has("tool:execute.before"));

	host.close();
	await cleanup();
});

test("v2: a catalog the transform cannot read never breaks the turn", async () => {
	const { exec } = recordingExec(engine());
	const host = fakeHost();
	const cleanup = await createPlugin({ exec, env: {} }).setup(host.ctx);

	// A measured edge: an uncaught throw inside this callback corrupts the
	// turn, so every shape of a catalog this plugin cannot read is swallowed
	// and the host keeps the definitions it built.
	for (const editor of [
		{ list() { throw new Error("no catalog"); } },
		{ list: () => null },
		{ list: () => [{ id: "mnexis_memory_save", description: "", options: {} }] },
		null,
	]) {
		assert.doesNotThrow(() => host.runTransform(editor));
	}

	host.close();
	await cleanup();
});

test("v2: the transform rewrites the description of every mnexis tool the map knows", async () => {
	const { exec } = recordingExec(engine());
	const host = fakeHost();
	const cleanup = await createPlugin({ exec, env: {} }).setup(host.ctx);

	const editor = host.transform([
		{
			id: "mnexis_memory_save",
			description: "Save a memory that must survive the session.",
			options: { namespace: "mnexis", codemode: true },
		},
		{
			id: "mnexis_memory_context",
			description: "Call this FIRST when you are about to work.",
			options: { namespace: "mnexis", codemode: true },
		},
		{ id: "mnexis_retired_tool", description: "The engine renamed this one.", options: {} },
		{ id: "bash", description: "Run a shell command." },
	]);

	assert.equal(
		editor.get("mnexis_memory_save").description,
		CURATED_DESCRIPTIONS.mnexis_memory_save,
	);
	assert.equal(
		editor.get("mnexis_memory_context").description,
		CURATED_DESCRIPTIONS.mnexis_memory_context,
	);

	// The seven kinds ride the save, one per line, exactly as pi states them.
	const save = editor.get("mnexis_memory_save").description.split("\n");
	assert.equal(TYPE_GLOSSES.length, 7);
	for (const gloss of TYPE_GLOSSES) {
		assert.ok(save.includes(gloss), `the save carries the gloss: ${gloss}`);
	}

	// An id the map does not know is skipped; a tool that is not mnexis's is
	// never touched.
	assert.equal(editor.get("mnexis_retired_tool").description, "The engine renamed this one.");
	assert.equal(editor.get("bash").description, "Run a shell command.");

	host.close();
	await cleanup();
});

test("v2: the transform turns Code Mode off for every mnexis tool, and nothing else", async () => {
	const { exec } = recordingExec(engine());
	const host = fakeHost();
	const cleanup = await createPlugin({ exec, env: {} }).setup(host.ctx);

	const editor = host.transform([
		{
			id: "mnexis_memory_save",
			description: "",
			options: { namespace: "mnexis", codemode: true },
		},
		{ id: "mnexis_memory_workspace", description: "" },
		{ id: "mnexis_retired_tool", description: "", options: { namespace: "mnexis" } },
		{ id: "bash", description: "Run a shell command.", options: { codemode: true } },
	]);

	// The host's own keys survive; only codemode moves.
	assert.deepEqual(editor.get("mnexis_memory_save").options, {
		namespace: "mnexis",
		codemode: false,
	});
	// A tool the host gave no options keeps working, with the one field.
	assert.deepEqual(editor.get("mnexis_memory_workspace").options, { codemode: false });
	// An id the map does not know is still mnexis's: it stays reachable.
	assert.deepEqual(editor.get("mnexis_retired_tool").options, {
		namespace: "mnexis",
		codemode: false,
	});
	// A tool that is not mnexis's gets no options invented for it.
	assert.deepEqual(editor.get("bash").options, { codemode: true });

	host.close();
	await cleanup();
});

test("v2: a catalog with no mnexis tools is left exactly as it is", async () => {
	const { exec } = recordingExec(engine());
	const host = fakeHost();
	const cleanup = await createPlugin({ exec, env: {} }).setup(host.ctx);

	const tools = [
		{ id: "bash", description: "Run a shell command.", options: { codemode: true } },
		{ id: "read", description: "Read a file." },
	];
	const editor = host.transform(tools);

	assert.deepEqual(editor.list(), tools);

	host.close();
	await cleanup();
});
