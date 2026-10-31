/**
 * S3 (audit A8): the engine's silence is never reported as "there is nothing".
 *
 * The audit measured three lies and one throw: "Three sites report an engine
 * silence as 'there is nothing' (`No memories matched/stored/answered`, and
 * `String(result.data)` → `'null'`), and ENOENT maps to exit 127 which
 * `EXIT_CODES` does not know → `'exited with code 127 (unknown error)'`; every
 * *write* tool throws instead of returning the actionable text."
 *
 * Every test here drives the tool the way pi does against a real child
 * process, so the assertion is the text the model reads: an engine that
 * printed nothing, printed something unreadable, or was never installed must
 * read as a failed call that names the next step — and a genuinely empty
 * answer must still read as the empty answer it is.
 */

import { test } from "node:test";
import assert from "node:assert/strict";
import { chmodSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import mnexisMemory from "../index.js";

/**
 * A fake engine whose answers the test chooses.
 *
 * MNEXIS_TEST_ANSWER stands for the engine's answer to everything but the
 * workspace resolution: "silent" (exit 0, no output), "unreadable" (exit 0,
 * output that is not JSON) or "ok". MNEXIS_TEST_CONFIG stands for the profile
 * read a write needs first: "ok", "fail" (a refusal with a reason) or
 * "silent".
 */
const FAKE_BINARY = `#!/usr/bin/env node
const args = process.argv.slice(2);
const key = "git:test-workspace";
const mode = process.env.MNEXIS_TEST_ANSWER || "ok";
const config = process.env.MNEXIS_TEST_CONFIG || "ok";
const send = (payload) => process.stdout.write(JSON.stringify(payload));

if (args[0] === "project" && args[1] === "list") {
	send({ bindings: [{ key, namespace: "mnexis" }], current_key: key });
} else if (args[0] === "config" && args[1] === "show") {
	if (config === "fail") {
		process.stderr.write("failed to read the profile");
		process.exit(1);
	}
	if (config !== "silent") {
		send({ scope: { write_namespaces: ["mnexis"] } });
	}
} else if (mode === "silent") {
	// Exit 0 and nothing on stdout: the engine said nothing at all.
} else if (mode === "unreadable") {
	process.stdout.write("mnexis: the store is locked by another process");
} else if (args[0] === "search" || args[0] === "prompts") {
	send({ count: 0, items: [] });
} else if (args[0] === "list") {
	send({ count: 0, items: [] });
} else if (args[0] === "context") {
	send({ items: [], searched_namespace: "mnexis", tokens_used: 0 });
} else if (args[0] === "show") {
	send({ uuid: args[1], namespace: "mnexis", revision: 1, state: "active" });
} else if (args[0] === "history") {
	send({ uuid: args[1], count: 0, items: [] });
} else if (args[0] === "recall") {
	send({ session: { id: "s1" }, memories_in_window: [] });
} else if (args[0] === "status") {
	send({ workspace: { key, namespace: "mnexis" }, memory_count: 0, recent: [] });
} else if (args[0] === "save") {
	send({ uuid: "u-1", row_id: 1, revision: 1, title: "el puerto", keywords: [] });
} else if (args[0] === "suggest-topic-key") {
	send({ title: args[1], topic_key: "puerto-del-servicio", recorded_by: null });
} else {
	send({});
}
`;

const ENV_KEYS = [
	"MNEXIS_BIN",
	"MNEXIS_TEST_ANSWER",
	"MNEXIS_TEST_CONFIG",
	"MNEXIS_TIMEOUT_MS",
];

/** Remember the environment so a test cannot leak into the next one. */
function snapshotEnv() {
	const saved = new Map(ENV_KEYS.map((key) => [key, process.env[key]]));
	return () => {
		for (const [key, value] of saved) {
			if (value === undefined) delete process.env[key];
			else process.env[key] = value;
		}
	};
}

/** Install the fake engine and point the extension at it. */
function setup() {
	const dir = mkdtempSync(join(tmpdir(), "mnexis-silence-"));
	const binary = join(dir, "mnexis");
	writeFileSync(binary, FAKE_BINARY);
	chmodSync(binary, 0o755);
	process.env.MNEXIS_BIN = binary;
	return { dir };
}

/** A host that records the tools the extension registers. */
function fakeHost() {
	const tools = new Map();
	return {
		tools,
		pi: {
			registerTool(tool) {
				tools.set(tool.name, tool);
			},
			on() {},
			appendEntry() {},
		},
	};
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

/** The text a tool answered. */
function textOf(result) {
	return (result?.content ?? [])
		.filter((entry) => entry && entry.type === "text")
		.map((entry) => entry.text)
		.join("\n");
}

test("a search the engine never answered is a failed call, not 'no memories matched'", async () => {
	const restore = snapshotEnv();
	const scratch = setup();
	process.env.MNEXIS_TEST_ANSWER = "silent";
	try {
		const { tools, pi } = fakeHost();
		mnexisMemory(pi);
		const result = await tools
			.get("mnexis_search")
			.execute("s1", { query: "puerto" }, undefined, undefined, context());

		const text = textOf(result);
		assert.equal(result.isError, true, `silence is a failure: ${text}`);
		assert.doesNotMatch(text, /No memories matched/, `it is not emptiness: ${text}`);
		assert.match(text, /did not answer/i, `it says the call failed: ${text}`);
		assert.match(text, /mnexis search/, `it names the call: ${text}`);
		assert.match(text, /terminal/, `it names the next step: ${text}`);
	} finally {
		restore();
		rmSync(scratch.dir, { recursive: true, force: true });
	}
});

test("a list the engine answered with unreadable output is a failed call, not an empty store", async () => {
	const restore = snapshotEnv();
	const scratch = setup();
	process.env.MNEXIS_TEST_ANSWER = "unreadable";
	try {
		const { tools, pi } = fakeHost();
		mnexisMemory(pi);
		const result = await tools
			.get("mnexis_list")
			.execute("s2", {}, undefined, undefined, context());

		const text = textOf(result);
		assert.equal(result.isError, true, `an unreadable answer is a failure: ${text}`);
		assert.doesNotMatch(text, /No memories stored yet/, `it is not emptiness: ${text}`);
		assert.match(text, /cannot read/i, `it says what went wrong: ${text}`);
		assert.match(text, /mnexis list/, `it names the call: ${text}`);
	} finally {
		restore();
		rmSync(scratch.dir, { recursive: true, force: true });
	}
});

test("a context the engine never answered is a failed call, not 'no memories answered'", async () => {
	const restore = snapshotEnv();
	const scratch = setup();
	process.env.MNEXIS_TEST_ANSWER = "silent";
	try {
		const { tools, pi } = fakeHost();
		mnexisMemory(pi);
		const result = await tools
			.get("mnexis_context")
			.execute("s3", { task: "el puerto" }, undefined, undefined, context());

		const text = textOf(result);
		assert.equal(result.isError, true, `silence is a failure: ${text}`);
		assert.doesNotMatch(text, /No memories answered/, `it is not emptiness: ${text}`);
		assert.match(text, /did not answer/i, `it says the call failed: ${text}`);
		assert.match(text, /mnexis context/, `it names the call: ${text}`);
	} finally {
		restore();
		rmSync(scratch.dir, { recursive: true, force: true });
	}
});

test("the four answers that fell back to 'null' say the call failed instead", async () => {
	const restore = snapshotEnv();
	const scratch = setup();
	process.env.MNEXIS_TEST_ANSWER = "silent";
	try {
		const { tools, pi } = fakeHost();
		mnexisMemory(pi);
		const calls = [
			["mnexis_show", { uuid: "11111111-1111-1111-1111-111111111111" }],
			["mnexis_history", { uuid: "11111111-1111-1111-1111-111111111111" }],
			["mnexis_recall", { previous: true }],
			["mnexis_workspace", {}],
		];
		for (const [name, params] of calls) {
			const result = await tools.get(name).execute("s4", params, undefined, undefined, context());
			const text = textOf(result);
			assert.equal(result.isError, true, `${name}: silence is a failure: ${text}`);
			assert.notEqual(text.trim(), "null", `${name} never answers 'null': ${text}`);
			assert.match(text, /did not answer/i, `${name} says the call failed: ${text}`);
		}
	} finally {
		restore();
		rmSync(scratch.dir, { recursive: true, force: true });
	}
});

test("a store the engine never answered is a failed call for the two reads that named no empty state", async () => {
	const restore = snapshotEnv();
	const scratch = setup();
	process.env.MNEXIS_TEST_ANSWER = "silent";
	try {
		const { tools, pi } = fakeHost();
		mnexisMemory(pi);
		for (const [name, params] of [
			["mnexis_search_prompts", { query: "puerto" }],
			["mnexis_suggest_topic_key", { title: "el puerto" }],
		]) {
			const result = await tools.get(name).execute("s5", params, undefined, undefined, context());
			const text = textOf(result);
			assert.equal(result.isError, true, `${name}: silence is a failure: ${text}`);
			assert.doesNotMatch(
				text,
				/no topic key|No prompts stored/,
				`${name} does not report the silence as an empty answer: ${text}`,
			);
			assert.match(text, /did not answer/i, `${name} says the call failed: ${text}`);
		}
	} finally {
		restore();
		rmSync(scratch.dir, { recursive: true, force: true });
	}
});

test("a supersede whose boundary read says nothing refuses without inventing an owner", async () => {
	const restore = snapshotEnv();
	const scratch = setup();
	process.env.MNEXIS_TEST_ANSWER = "silent";
	try {
		const { tools, pi } = fakeHost();
		mnexisMemory(pi);
		const result = await tools
			.get("mnexis_supersede")
			.execute(
				"s11",
				{ older: "u-old", newer: "u-new", reason: "el puerto cambió" },
				undefined,
				undefined,
				context(),
			);

		const text = textOf(result);
		assert.equal(result.isError, true, `a boundary read that failed refuses the write: ${text}`);
		assert.doesNotMatch(text, /namespace 'unknown'/, `no owner is invented: ${text}`);
		assert.match(text, /Cannot read the memory u-old/, `it says what could not be read: ${text}`);
		assert.match(text, /nothing was changed/, `it says nothing was written: ${text}`);
	} finally {
		restore();
		rmSync(scratch.dir, { recursive: true, force: true });
	}
});

test("a binary that is not installed is a failed call that names the way back, not 'unknown error'", async () => {	const restore = snapshotEnv();
	process.env.MNEXIS_BIN = "/nonexistent/mnexis-engine-for-the-test";
	try {
		const { tools, pi } = fakeHost();
		mnexisMemory(pi);
		const result = await tools
			.get("mnexis_search")
			.execute("s6", { query: "puerto" }, undefined, undefined, context());

		const text = textOf(result);
		assert.equal(result.isError, true, `a missing engine is a failure: ${text}`);
		assert.doesNotMatch(text, /unknown error/, `the unknown is gone: ${text}`);
		assert.match(text, /127|not found|could not run/i, `it says what happened: ${text}`);
		assert.match(text, /MNEXIS_BIN/, `it names the way back: ${text}`);
	} finally {
		restore();
	}
});

test("a save the engine answers nothing to is a failed call, and says the write may still have landed", async () => {
	const restore = snapshotEnv();
	const scratch = setup();
	process.env.MNEXIS_TEST_ANSWER = "silent";
	try {
		const { tools, pi } = fakeHost();
		mnexisMemory(pi);
		const result = await tools
			.get("mnexis_save")
			.execute("s7", { content: "el puerto es 8081", title: "el puerto" }, undefined, undefined, context());

		const text = textOf(result);
		assert.equal(result.isError, true, `silence is not a save: ${text}`);
		assert.doesNotMatch(text, /Saved a memory/, `it never claims the save happened: ${text}`);
		assert.match(text, /did not answer/i, `it says the call failed: ${text}`);
		assert.match(text, /may still have landed/, `it is honest about the write: ${text}`);
		assert.match(text, /mnexis_search/, `it names the way to check: ${text}`);
	} finally {
		restore();
		rmSync(scratch.dir, { recursive: true, force: true });
	}
});

test("a write whose profile read fails returns the reason instead of throwing", async () => {
	const restore = snapshotEnv();
	const scratch = setup();
	process.env.MNEXIS_TEST_CONFIG = "fail";
	try {
		const { tools, pi } = fakeHost();
		mnexisMemory(pi);
		const result = await tools
			.get("mnexis_save")
			.execute("s8", { content: "el puerto es 8081", title: "el puerto" }, undefined, undefined, context());

		const text = textOf(result);
		assert.equal(result.isError, true, `a failed profile read is a failed write: ${text}`);
		assert.match(text, /failed to read the profile/, `the engine's reason reaches the model: ${text}`);
		assert.match(text, /No memory was written/, `it says nothing was written: ${text}`);
	} finally {
		restore();
		rmSync(scratch.dir, { recursive: true, force: true });
	}
});

test("a write whose profile read answers nothing returns the failure instead of throwing", async () => {
	const restore = snapshotEnv();
	const scratch = setup();
	process.env.MNEXIS_TEST_CONFIG = "silent";
	try {
		const { tools, pi } = fakeHost();
		mnexisMemory(pi);
		const result = await tools
			.get("mnexis_save")
			.execute("s9", { content: "el puerto es 8081", title: "el puerto" }, undefined, undefined, context());

		const text = textOf(result);
		assert.equal(result.isError, true, `silence is a failed write: ${text}`);
		assert.doesNotMatch(text, /Saved a memory/, `it never claims the save happened: ${text}`);
		assert.match(text, /did not answer/i, `it says the call failed: ${text}`);
		assert.match(text, /No memory was written/, `it says nothing was written: ${text}`);
	} finally {
		restore();
		rmSync(scratch.dir, { recursive: true, force: true });
	}
});

test("a genuinely empty answer still reads as the empty answer it is", async () => {
	const restore = snapshotEnv();
	const scratch = setup();
	process.env.MNEXIS_TEST_ANSWER = "ok";
	try {
		const { tools, pi } = fakeHost();
		mnexisMemory(pi);

		const searched = await tools
			.get("mnexis_search")
			.execute("s10", { query: "puerto" }, undefined, undefined, context());
		assert.notEqual(searched.isError, true);
		assert.match(textOf(searched), /No memories matched 'puerto'/);

		const listed = await tools.get("mnexis_list").execute("s10", {}, undefined, undefined, context());
		assert.notEqual(listed.isError, true);
		assert.match(textOf(listed), /No memories stored yet/);

		const answered = await tools
			.get("mnexis_context")
			.execute("s10", { task: "el puerto" }, undefined, undefined, context());
		assert.notEqual(answered.isError, true);
		assert.match(textOf(answered), /No memories answered the task 'el puerto'/);
	} finally {
		restore();
		rmSync(scratch.dir, { recursive: true, force: true });
	}
});
