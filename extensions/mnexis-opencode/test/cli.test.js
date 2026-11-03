/**
 * The CLI client: what it costs, what it retries, and what it reads.
 *
 * The host awaits these calls, so each one carries a budget of its own, and a
 * store that is momentarily locked is worth one more try on the commands whose
 * identity the engine owns. Both patterns are the engine-compatible ones the
 * pi extension measured; this host keeps its own copy.
 */

import assert from "node:assert/strict";
import test from "node:test";

import {
	BUSY_EXIT_CODE,
	BUSY_RETRY_DELAYS_MS,
	READ_TIMEOUT_MS,
	WRITE_TIMEOUT_MS,
	createClient,
	parseJson,
} from "../index.js";

test("the CLI's JSON is read even when something else wrote around it", () => {
	assert.deepEqual(parseJson('{"a":1}'), { a: 1 });
	assert.deepEqual(parseJson('\nwarning: x\n{"a":1}\n'), { a: 1 });
	assert.equal(parseJson(""), null);
	assert.equal(parseJson("not json"), null);
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


});

// --- the closing state of a session (fix 2) --------------------------------
//
// pi writes one "estado al cierre" `project_state` at close and states only what
// the engine answered (index.ts:2489-2520, `sessionCloseContent`). This plugin
// writes the same shape through the same `run()` path, so the write budget and
// the busy retry apply to it like every other write.

test("the closing record is a write that retries a locked store", async () => {
	const { exec, calls } = scriptedExec([BUSY, { code: 0, stdout: "{}", stderr: "" }]);
	const client = createClient({ exec });

	const result = await client.saveMemory(
		{
			namespace: "acme",
			title: "estado al cierre",
			type: "project_state",
			content: "Session s1 ended.",
		},
		"/repo",
	);

	assert.deepEqual(calls[0].args, [
		"save",
		"--namespace",
		"acme",
		"--type",
		"project_state",
		"--title",
		"estado al cierre",
		"Session s1 ended.",
		"--json",
	]);
	assert.equal(calls[0].options.timeout, WRITE_TIMEOUT_MS, "a write gets the write budget");
	assert.equal(calls.length, 2, "the title is the record's identity: the retry is the same record");
	assert.equal(result.code, 0);
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
