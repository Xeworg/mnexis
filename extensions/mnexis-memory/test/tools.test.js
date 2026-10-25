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
import { execFileSync } from "node:child_process";

// The canonical key of a workspace, the way the engine computes it: the
// normalized remote when the workspace is a repository, its path otherwise.
function canonicalKey(cwd) {
	try {
		const root = execFileSync("git", ["rev-parse", "--show-toplevel"], { cwd, encoding: "utf8" }).trim();
		const remote = execFileSync("git", ["config", "--get", "remote.origin.url"], { cwd: root, encoding: "utf8" }).trim();
		if (remote) {
			const normalized = remote
				.replace(/^git\\+/i, "")
				.replace(/^[a-z][a-z0-9+.-]*:\\/\\//i, "")
				.replace(/^[^/@]+@/, "")
				.replace(/\\.git$/i, "")
				.toLowerCase()
				.replace(/[/:]+/g, "-")
				.replace(/-+/g, "-")
				.replace(/^-+|-+$/g, "");
			return "git:" + normalized;
		}
		return "path:" + root;
	} catch {
		return "path:" + cwd;
	}
}
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
	// The engine publishes the canonical key of the workspace it runs in
	// and the binding for it. MNEXIS_FAKE_NO_BINDING stands for a
	// workspace nobody bound yet.
	const key = process.env.MNEXIS_FAKE_PUBLISHED_KEY || canonicalKey(process.cwd());
	const bindings = process.env.MNEXIS_FAKE_NO_BINDING ? [] : [{ key, namespace: workspace }];
	process.stdout.write(JSON.stringify({ bindings, current_key: key }));
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
} else if (args[0] === "list") {
	const listPayload = {
		count: 1,
		items: [{ uuid: "m1", namespace: "payments", preview: "el puerto es 8081" }],
		relations: [{ older: "m1", newer: "m2", type: "supersedes", reason: "el puerto cambió" }],
	};
	if (process.env.MNEXIS_FAKE_READ_SCOPE) {
		listPayload.read_scope = process.env.MNEXIS_FAKE_READ_SCOPE;
	}
	process.stdout.write(JSON.stringify(listPayload));
} else if (args[0] === "save") {
	// The words the engine answers with: the ones this save asked for, plus
	// the ones the record already carried. MNEXIS_FAKE_KEYWORDS stands for a
	// record a previous session wrote.
	const requested = [];
	for (let i = 0; i < args.length; i += 1) {
		if (args[i] === "--keywords") {
			requested.push(
				...String(args[i + 1])
					.split(",")
					.map((word) => word.trim())
					.filter(Boolean),
			);
		}
	}
	const accumulated = process.env.MNEXIS_FAKE_KEYWORDS
		? JSON.parse(process.env.MNEXIS_FAKE_KEYWORDS)
		: requested;
	process.stdout.write(
		JSON.stringify({ uuid: "u-1", row_id: 1, keywords: accumulated }),
	);
} else if (args[0] === "search" || args[0] === "prompts") {
	const scope = process.env.MNEXIS_FAKE_READ_SCOPE;
	const payload = { count: 0, items: [] };
	if (scope) {
		payload.read_scope = scope;
	}
	process.stdout.write(JSON.stringify(payload));
} else if (args[0] === "suggest-topic-key") {
	process.stdout.write(JSON.stringify({ title: args[2], topic_key: "puerto-del-servicio", recorded_by: null }));
} else if (args[0] === "session" || args[0] === "end" || args[0] === "prompt") {
	process.stdout.write(JSON.stringify({ id: args[3] || "s1", session_id: args[3] || "s1" }));
} else if (args[0] === "recall") {
	// The priced payload the engine answers with: identity and preview,
	// the whole records only with --full.
	if (process.env.MNEXIS_FAKE_RECALL_FAIL) {
		process.stderr.write(process.env.MNEXIS_FAKE_RECALL_FAIL);
		process.exit(3);
	}
	process.stdout.write(
		JSON.stringify({
			session: { id: "01JX", namespace: workspace, summary: "el puerto del servicio" },
			branch_state: null,
			prompts: [],
			window: { namespace: workspace, open: false, label: "the memories of the namespace in the session's window" },
			memories_in_window: [
				{ uuid: "m1", title: "el puerto del servicio", namespace: workspace, revision: 1, preview: "el puerto es 8081" },
			],
		}),
	);
} else if (args[0] === "status") {
	process.stdout.write(
		JSON.stringify({
			workspace: { key: "git:github.com-xeworg-mnexis", namespace: workspace },
			scope: { default_namespace: "default", write_namespaces: [workspace] },
			memory_count: 3,
			recent: [
				{
					uuid: "m1",
					namespace: workspace,
					title: "puerto del servicio",
					keywords: [],
					memory_type: "fact",
					state: "active",
					revision: 1,
					topic_key: "puerto-del-servicio",
					created_at: "2026-01-01T00:00:00Z",
					updated_at: "2026-01-01T00:00:00Z",
					preview: "el puerto es 8081",
				},
			],
		}),
	);
} else if (args[0] === "history") {
	if (process.env.MNEXIS_FAKE_HISTORY_FAIL) {
		process.stderr.write(process.env.MNEXIS_FAKE_HISTORY_FAIL);
		process.exit(3);
	}
	process.stdout.write(
		JSON.stringify({
			uuid: args[1],
			count: 1,
			items: [{ revision: 1, state: "active", content: "el puerto es 8081", changed_by: "user", changed_at: "2026-10-26T00:00:00Z" }],
		}),
	);
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

test("the search tool can ask for a match mode", async () => {
	const { log } = setup();
	const { tools, pi } = fakeHost();
	mnexisMemory(pi);
	const search = tools.get("mnexis_search");
	assert.ok(search, "mnexis_search is registered");

	await search.execute("t1b", { query: "puerto", match: "all" }, undefined, undefined, context());

	const searchCall = calls(log).find((args) => args[0] === "search");
	assert.ok(searchCall, "search was called");
	assert.ok(searchCall.includes("--match"), `the mode reaches the CLI: ${JSON.stringify(searchCall)}`);
	assert.ok(searchCall.includes("all"), `the mode is the one asked for: ${JSON.stringify(searchCall)}`);
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
	process.env.MNEXIS_FAKE_NO_BINDING = "1";
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
	delete process.env.MNEXIS_FAKE_NO_BINDING;
});

test("a workspace with no binding refuses the write and names the commands", async () => {
	const { log } = setup();
	process.env.MNEXIS_FAKE_NO_BINDING = "1";
	const { tools, pi } = fakeHost();
	mnexisMemory(pi);
	const save = tools.get("mnexis_save");
	assert.ok(save, "mnexis_save is registered");

	const result = await save.execute("t8", { content: "a fact" }, undefined, undefined, {
		...context(),
		cwd: process.cwd(),
	});
	assert.equal(result.isError, true, "the write is refused");
	const text = result.content.map((block) => block.text).join(" ");
	assert.match(text, /mnexis project bind/, "the message names the bind command");
	assert.match(text, /mnexis config allow-namespace/, "the message names the allow command");

	const bind = calls(log).find((args) => args[0] === "project" && args[1] === "bind");
	assert.equal(bind, undefined, "the agent does not invent a namespace");
	delete process.env.MNEXIS_FAKE_NO_BINDING;
});

test("the engine's published key is the one the extension binds and looks up", async () => {
	const { log } = setup();
	process.env.MNEXIS_FAKE_PUBLISHED_KEY = "git:engine-owned-key";
	process.env.MNEXIS_FAKE_NO_BINDING = "1";
	const { tools, pi } = fakeHost();
	mnexisMemory(pi);
	const save = tools.get("mnexis_save");
	assert.ok(save, "mnexis_save is registered");

	const result = await save.execute("t9", { content: "a fact" }, undefined, undefined, {
		...context(),
		cwd: process.cwd(),
	});
	assert.equal(result.isError, true, "no binding under the engine key yet");
	const text = result.content.map((block) => block.text).join(" ");
	assert.match(text, /git:engine-owned-key/, `the message must carry the ENGINE key: ${text}`);
	assert.equal(
		calls(log).find((args) => args[0] === "project" && args[1] === "bind"),
		undefined,
		"the agent does not bind a namespace of its own",
	);
	delete process.env.MNEXIS_FAKE_PUBLISHED_KEY;
	delete process.env.MNEXIS_FAKE_NO_BINDING;
});

test("the session tools reach the engine", async () => {
	const { log } = setup();
	const { tools, pi } = fakeHost();
	mnexisMemory(pi);

	const start = tools.get("mnexis_session_start");
	assert.ok(start, "mnexis_session_start is registered");
	const started = await start.execute(
		"s1",
		{ session_id: "01JX", directory: "/work/project" },
		undefined,
		undefined,
		context(),
	);
	assert.notEqual(started.isError, true);
	const startCall = calls(log).find((args) => args[0] === "session");
	assert.ok(startCall, "the session was registered");
	assert.ok(startCall.includes("01JX"), `the id reaches the engine: ${JSON.stringify(startCall)}`);

	const end = tools.get("mnexis_session_end");
	assert.ok(end, "mnexis_session_end is registered");
	await end.execute(
		"s2",
		{ session_id: "01JX", summary: "what the session did" },
		undefined,
		undefined,
		context(),
	);
	const endCall = calls(log).find((args) => args[0] === "end");
	assert.ok(endCall, "the session was ended");
	assert.ok(endCall.includes("what the session did"), "the summary reaches the engine");
});

test("the prompt tools reach the engine", async () => {
	const { log } = setup();
	const { tools, pi } = fakeHost();
	mnexisMemory(pi);

	const save = tools.get("mnexis_save_prompt");
	assert.ok(save, "mnexis_save_prompt is registered");
	await save.execute(
		"s3",
		{ content: "el puerto es 8081", session_id: "01JX" },
		undefined,
		undefined,
		context(),
	);
	const promptCall = calls(log).find((args) => args[0] === "prompt");
	assert.ok(promptCall, "the prompt was stored");
	assert.ok(promptCall.includes("el puerto es 8081"), "the content reaches the engine");

	const search = tools.get("mnexis_search_prompts");
	assert.ok(search, "mnexis_search_prompts is registered");
	await search.execute("s4", { query: "puerto 8081" }, undefined, undefined, context());
	const searchCall = calls(log).find((args) => args[0] === "prompts");
	assert.ok(searchCall, "the prompts were searched");
	assert.ok(searchCall.includes("--search"), "it is a search");
});

test("the recall reaches the engine in one call", async () => {
	const { log } = setup();
	const { tools, pi } = fakeHost();
	mnexisMemory(pi);

	const recall = tools.get("mnexis_recall");
	assert.ok(recall, "mnexis_recall is registered");

	const result = await recall.execute("r1", { previous: true }, undefined, undefined, context());
	assert.notEqual(result.isError, true);
	const recallCall = calls(log).find((args) => args[0] === "recall");
	assert.ok(recallCall, "the recall was called");
	assert.deepEqual(recallCall, ["recall", "--previous", "--json"]);

	// The answer the engine gave is the answer the model reads: the priced
	// payload, identity and preview, in one call.
	const text = String(result.content?.[0]?.text ?? "");
	assert.match(text, /01JX/, `the session is in the answer: ${text}`);
	assert.match(text, /preview/, `the memories are previews: ${text}`);
	assert.doesNotMatch(text, /"content"/, `no full content by default: ${text}`);
});

test("the recall by id and the full flag reach the CLI", async () => {
	const { log } = setup();
	const { tools, pi } = fakeHost();
	mnexisMemory(pi);
	const recall = tools.get("mnexis_recall");
	assert.ok(recall, "mnexis_recall is registered");

	await recall.execute("r2", { session: "01JX" }, undefined, undefined, context());
	const byId = calls(log).filter((args) => args[0] === "recall").pop();
	assert.deepEqual(byId, ["recall", "--session", "01JX", "--json"]);

	await recall.execute("r3", { previous: true, full: true }, undefined, undefined, context());
	const full = calls(log).filter((args) => args[0] === "recall").pop();
	assert.deepEqual(full, ["recall", "--previous", "--full", "--json"]);
});

test("a recall with neither session nor previous refuses to guess", async () => {
	const { log } = setup();
	const { tools, pi } = fakeHost();
	mnexisMemory(pi);
	const recall = tools.get("mnexis_recall");
	assert.ok(recall, "mnexis_recall is registered");

	const result = await recall.execute("r4", {}, undefined, undefined, context());
	assert.equal(result.isError, true, "it must refuse");
	const text = String(result.content?.[0]?.text ?? "");
	assert.match(text, /session/, `the refusal names what to give: ${text}`);
	assert.equal(
		calls(log).some((args) => args[0] === "recall"),
		false,
		"nothing reached the engine",
	);
});

test("a failed recall surfaces the engine's answer", async () => {
	const { log } = setup();
	process.env.MNEXIS_FAKE_RECALL_FAIL =
		"NOT_FOUND: no finished session in namespace mnexis: no finished session yet";
	try {
		const { tools, pi } = fakeHost();
	mnexisMemory(pi);
		const recall = tools.get("mnexis_recall");
		const result = await recall.execute("r5", { previous: true }, undefined, undefined, context());
		assert.equal(result.isError, true, "it must fail");
		const text = String(result.content?.[0]?.text ?? "");
		assert.match(text, /no finished session/, `the reason reaches the model: ${text}`);
	} finally {
		delete process.env.MNEXIS_FAKE_RECALL_FAIL;
	}
});

test("the workspace answers where the session may write, in one call", async () => {
	const { log } = setup();
	const { tools, pi } = fakeHost();
	mnexisMemory(pi);

	const workspace = tools.get("mnexis_workspace");
	assert.ok(workspace, "mnexis_workspace is registered");

	const result = await workspace.execute("w1", {}, undefined, undefined, context());
	assert.notEqual(result.isError, true);
	const statusCall = calls(log).find((args) => args[0] === "status");
	assert.ok(statusCall, "the status was asked");
	assert.deepEqual(statusCall, ["status", "--json"]);
	const text = String(result.content?.[0]?.text ?? "");
	assert.match(text, /git:github.com-xeworg-mnexis/, `the workspace key is in the answer: ${text}`);
	assert.match(text, /memory_count/, `the count is in the answer: ${text}`);
	// The recent list is priced: the answer names each memory and previews
	// it, and never pastes the whole record (#53).
	assert.match(text, /puerto del servicio/, `the identity reaches the model: ${text}`);
	assert.match(text, /el puerto es 8081/, `the preview reaches the model: ${text}`);
	assert.doesNotMatch(text, /"content"/, `the full record is not in the answer: ${text}`);
});

test("the history of one memory reaches the engine", async () => {
	const { log } = setup();
	const { tools, pi } = fakeHost();
	mnexisMemory(pi);

	const history = tools.get("mnexis_history");
	assert.ok(history, "mnexis_history is registered");

	const result = await history.execute("h1", { uuid: "m1" }, undefined, undefined, context());
	assert.notEqual(result.isError, true);
	const historyCall = calls(log).find((args) => args[0] === "history");
	assert.ok(historyCall, "the history was asked");
	assert.deepEqual(historyCall, ["history", "m1", "--json"]);

	await history.execute("h2", { uuid: "m1", limit: 5 }, undefined, undefined, context());
	const limited = calls(log).filter((args) => args[0] === "history").pop();
	assert.deepEqual(limited, ["history", "m1", "--limit", "5", "--json"]);
});

test("a history the engine cannot find surfaces the reason", async () => {
	const { log } = setup();
	process.env.MNEXIS_FAKE_HISTORY_FAIL = "NOT_FOUND: not found: memory NO-EXISTE";
	try {
		const { tools, pi } = fakeHost();
	mnexisMemory(pi);
		const history = tools.get("mnexis_history");
		const result = await history.execute("h3", { uuid: "NO-EXISTE" }, undefined, undefined, context());
		assert.equal(result.isError, true, "it must fail");
		const text = String(result.content?.[0]?.text ?? "");
		assert.match(text, /NO-EXISTE/, `the reason reaches the model: ${text}`);
	} finally {
		delete process.env.MNEXIS_FAKE_HISTORY_FAIL;
	}
});

test("a session id is never invented", async () => {
	const { log } = setup();
	const { tools, pi } = fakeHost();
	mnexisMemory(pi);
	const start = tools.get("mnexis_session_start");
	const result = await start.execute(
		"s5",
		{},
		undefined,
		undefined,
		{ ...context(), sessionManager: undefined },
	);
	assert.equal(result.isError, true, "without an id there is nothing to register");
	assert.equal(
		calls(log).find((args) => args[0] === "session"),
		undefined,
		"the engine is not called with a made-up id",
	);
});

test("the topic suggestion reaches the engine", async () => {
	const { log } = setup();
	const { tools, pi } = fakeHost();
	mnexisMemory(pi);
	const suggest = tools.get("mnexis_suggest_topic_key");
	assert.ok(suggest, "mnexis_suggest_topic_key is registered");

	await suggest.execute(
		"s6",
		{ title: "puerto del servicio" },
		undefined,
		undefined,
		context(),
	);
	const call = calls(log).find((args) => args[0] === "suggest-topic-key");
	assert.ok(call, "the engine was asked");
	assert.ok(call.includes("puerto del servicio"), "the title reaches the engine");
});

test("the topic suggestion answers in the workspace namespace", async () => {
	const { log } = setup();
	const { tools, pi } = fakeHost();
	mnexisMemory(pi);
	const suggest = tools.get("mnexis_suggest_topic_key");
	assert.ok(suggest, "mnexis_suggest_topic_key is registered");

	// The workspace is bound to a namespace; the model names none. The
	// call must not fall back to the profile default (which the workspace
	// guard refuses) and must not need the write confirmation: it is a read.
	const result = await suggest.execute(
		"s8",
		{ title: "puerto del servicio" },
		undefined,
		undefined,
		context(),
	);
	assert.notEqual(result.isError, true, "a read does not go through the write gate");

	const call = calls(log).find((args) => args[0] === "suggest-topic-key");
	assert.ok(call, "the engine was asked");
	assert.deepEqual(call, [
		"suggest-topic-key",
		"puerto del servicio",
		"--namespace",
		"mnexis",
		"--json",
	]);
});

test("an explicit namespace still wins for the topic suggestion", async () => {
	const { log } = setup();
	const { tools, pi } = fakeHost();
	mnexisMemory(pi);
	const suggest = tools.get("mnexis_suggest_topic_key");
	assert.ok(suggest, "mnexis_suggest_topic_key is registered");

	await suggest.execute(
		"s9",
		{ title: "puerto del servicio", namespace: "otro-proyecto" },
		undefined,
		undefined,
		context(),
	);

	const call = calls(log).find((args) => args[0] === "suggest-topic-key");
	assert.ok(call, "the engine was asked");
	assert.deepEqual(call, [
		"suggest-topic-key",
		"puerto del servicio",
		"--namespace",
		"otro-proyecto",
		"--json",
	]);
});

test("the save carries the title the agent gave", async () => {
	const { log } = setup();
	const { tools, pi } = fakeHost();
	mnexisMemory(pi);
	const save = tools.get("mnexis_save");
	assert.ok(save, "mnexis_save is registered");

	await save.execute(
		"s6b",
		{ content: "el puerto es 8081", title: "puerto del servicio" },
		undefined,
		undefined,
		context(),
	);
	const call = calls(log).find((args) => args[0] === "save");
	assert.ok(call, "the engine was asked");
	assert.ok(call.includes("--title"), `the title reaches the engine: ${JSON.stringify(call)}`);
	assert.ok(call.includes("puerto del servicio"), "and it is the title the agent gave");
});

test("the relations travel with the list the model reads", async () => {
	const { log } = setup();
	const { tools, pi } = fakeHost();
	mnexisMemory(pi);
	const list = tools.get("mnexis_list");
	assert.ok(list, "mnexis_list is registered");

	const result = await list.execute("s7", {}, undefined, undefined, context());
	assert.notEqual(result.isError, true);
	const text = result.content.map((block) => block.text).join(" ");
	assert.match(text, /was replaced by/, `the relation is in the text the model reads: ${text}`);
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

test("the save declares the words it accepts", async () => {
	setup();
	const { tools, pi } = fakeHost();
	mnexisMemory(pi);
	const save = tools.get("mnexis_save");
	assert.ok(save, "mnexis_save is registered");

	const keywords = save.parameters.properties.keywords;
	assert.ok(keywords, `the tool declares a keywords parameter: ${JSON.stringify(save.parameters)}`);
	assert.equal(keywords.type, "string", "the words arrive comma-separated, like tags");
});

test("the save passes the words to the engine", async () => {
	const { log } = setup();
	const { tools, pi } = fakeHost();
	mnexisMemory(pi);
	const save = tools.get("mnexis_save");

	await save.execute(
		"k1",
		{ content: "el puerto es 8081", keywords: "puerto, port" },
		undefined,
		undefined,
		context(),
	);
	const call = calls(log).find((args) => args[0] === "save");
	assert.ok(call, "the engine was asked");
	assert.ok(call.includes("--keywords"), `the words reach the engine: ${JSON.stringify(call)}`);
	assert.equal(
		call[call.indexOf("--keywords") + 1],
		"puerto, port",
		`and they are the words the agent gave: ${JSON.stringify(call)}`,
	);
});

test("the save message shows the accumulated words", async () => {
	const { log } = setup();
	// The record already carries a vocabulary a previous session wrote; this
	// save only names one word, so a message that showed the request instead
	// of the answer could not contain `puerto`.
	process.env.MNEXIS_FAKE_KEYWORDS = JSON.stringify(["listen port", "puerto"]);
	const { tools, pi } = fakeHost();
	mnexisMemory(pi);
	const save = tools.get("mnexis_save");

	const result = await save.execute(
		"k2",
		{ content: "el puerto es 8081", keywords: "listen port" },
		undefined,
		undefined,
		context(),
	);
	assert.notEqual(result.isError, true);
	const text = result.content.map((block) => block.text).join(" ");
	assert.match(text, /puerto/, `the accumulated words are in the message: ${text}`);
	assert.match(text, /listen port/, `the new word is there too: ${text}`);
	delete process.env.MNEXIS_FAKE_KEYWORDS;

	const call = calls(log).find((args) => args[0] === "save");
	assert.ok(call, "the engine was asked");
});

test("a save with no words says nothing about them", async () => {
	const { log } = setup();
	const { tools, pi } = fakeHost();
	mnexisMemory(pi);
	const save = tools.get("mnexis_save");

	const result = await save.execute(
		"k3",
		{ content: "sin palabras" },
		undefined,
		undefined,
		context(),
	);
	assert.notEqual(result.isError, true);
	const text = result.content.map((block) => block.text).join(" ");
	assert.doesNotMatch(text, /words/i, `nothing to report: ${text}`);

	const call = calls(log).find((args) => args[0] === "save");
	assert.ok(call, "the engine was asked");
	assert.equal(
		call.includes("--keywords"),
		false,
		`no words means no flag: ${JSON.stringify(call)}`,
	);
});

test("a bounded read tells the agent the scope it was bounded by", async () => {
	const { log } = setup();
	process.env.MNEXIS_FAKE_READ_SCOPE = "restricted";
	try {
		const { tools, pi } = fakeHost();
		mnexisMemory(pi);
		const search = tools.get("mnexis_search");
		assert.ok(search, "mnexis_search is registered");

		const result = await search.execute("t-scope", { query: "puerto" }, undefined, undefined, context());
		assert.notEqual(result.isError, true);
		const text = String(result.content?.[0]?.text ?? result.output ?? "");
		assert.match(
			text,
			/Read scope: restricted/,
			`the answer must say it was bounded: ${text}`,
		);
		assert.equal(
			result.details?.read_scope,
			"restricted",
			`and the payload carries the mode: ${JSON.stringify(result.details)}`,
		);
	} finally {
		delete process.env.MNEXIS_FAKE_READ_SCOPE;
	}
});

test("an open read says so too", async () => {
	const { log } = setup();
	process.env.MNEXIS_FAKE_READ_SCOPE = "all";
	try {
		const { tools, pi } = fakeHost();
		mnexisMemory(pi);
		const list = tools.get("mnexis_list");
		assert.ok(list, "mnexis_list is registered");

		const result = await list.execute("t-scope-all", {}, undefined, undefined, context());
		assert.notEqual(result.isError, true);
		const text = String(result.content?.[0]?.text ?? result.output ?? "");
		assert.match(text, /Read scope: all/, `the answer must say it was open: ${text}`);
	} finally {
		delete process.env.MNEXIS_FAKE_READ_SCOPE;
	}
});
