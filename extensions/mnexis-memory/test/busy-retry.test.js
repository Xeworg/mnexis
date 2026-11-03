/**
 * The busy retry: a save that finds the store locked is tried again.
 *
 * The engine waits two seconds on the lock itself and then gives up
 * (`src/storage/sqlite.rs:35`), and the extension had no second try: the
 * model decided the fact deserved to survive, the engine answered "busy",
 * and the fact was lost. The trim-nudge bench measured eight such losses in
 * thirty runs.
 *
 * The retry is narrow on purpose: only a save, and only one that names its
 * record. The title (or the key it was given) is what makes the second
 * attempt the same record instead of a second one — a save with no title
 * creates a new record every time, so retrying it would duplicate the fact.
 */

import { test } from "node:test";
import assert from "node:assert/strict";
import {
	BUSY_EXIT_CODE,
	BUSY_RETRY_DELAYS_MS,
	WRITE_TIMEOUT_MS,
	createClient,
} from "../lib/cli.ts";

/** The engine's answer when the store was locked. */
const BUSY = { code: BUSY_EXIT_CODE, stdout: "", stderr: "busy: sql: database is locked" };

/** A save that names its record: the title is the record's identity. */
const NAMED = { namespace: "payments", content: "a fact", title: "Timeout del gateway" };

/** Fake exec: records calls and replays scripted answers. */
function fakeExec(answers = []) {
	const calls = [];
	const exec = async (command, args, options) => {
		calls.push({ command, args, options });
		const answer = answers.shift();
		if (!answer) {
			return { code: 0, stdout: "{}", stderr: "" };
		}
		return answer;
	};
	return { exec, calls };
}

test("a save that finds the store locked is tried again, and the second answer is kept", async () => {
	const { exec, calls } = fakeExec([BUSY, { code: 0, stdout: '{"uuid":"u1"}', stderr: "" }]);
	const client = createClient({ exec });

	const result = await client.save(NAMED, "/work");

	assert.equal(calls.length, 2, `the save was tried twice: ${JSON.stringify(calls)}`);
	assert.equal(result.code, 0, "the answer that came back is the one that is kept");
	assert.equal(result.data.uuid, "u1");
	assert.equal(result.busy_retries, 1, "the answer says how many tries it took");
});

test("a locked save is tried at most twice more, and then reports the lock", async () => {
	const { exec, calls } = fakeExec([BUSY, BUSY, BUSY, BUSY]);
	const client = createClient({ exec });

	const result = await client.save(NAMED, "/work");

	assert.equal(calls.length, 1 + BUSY_RETRY_DELAYS_MS.length, "the tries are bounded");
	assert.equal(result.code, BUSY_EXIT_CODE, "a store that stays locked is still a failure");
	assert.equal(result.busy_retries, BUSY_RETRY_DELAYS_MS.length);
});

test("a locked save that names no record is not tried again", async () => {
	const { exec, calls } = fakeExec([BUSY]);
	const client = createClient({ exec });

	const result = await client.save({ namespace: "payments", content: "a fact" }, "/work");

	assert.equal(calls.length, 1, "a save with no title would write a second record: it is not retried");
	assert.equal(result.code, BUSY_EXIT_CODE);
	assert.equal(result.busy_retries, 0);
});

test("a save that names its record by key or by idempotency key is retried", async () => {
	for (const named of [
		{ namespace: "payments", content: "a fact", topicKey: "payments/gateway" },
		{ namespace: "payments", content: "a fact", idempotencyKey: "k-1" },
	]) {
		const { exec, calls } = fakeExec([BUSY, { code: 0, stdout: "{}", stderr: "" }]);
		const client = createClient({ exec });

		await client.save(named, "/work");

		assert.equal(calls.length, 2, `a save that names its record is retried: ${JSON.stringify(named)}`);
	}
});

test("a save whose title yields no key is not tried again", async () => {
	const { exec, calls } = fakeExec([BUSY]);
	const client = createClient({ exec });

	// The engine's key comes from the title, and a title of only punctuation
	// yields no key (`src/domain/title.rs`): that save writes a new record
	// every time, so trying it again would duplicate the fact.
	const result = await client.save(
		{ namespace: "payments", content: "a fact", title: "!!!" },
		"/work",
	);

	assert.equal(calls.length, 1, "a title that yields no key does not name a record");
	assert.equal(result.code, BUSY_EXIT_CODE);
});

test("a locked call that is not a save is not tried again", async () => {
	const { exec, calls } = fakeExec([BUSY]);
	const client = createClient({ exec });

	const result = await client.list({ namespace: "payments" }, "/work");

	assert.equal(calls.length, 1, "only a save is retried: ${JSON.stringify(calls)}");
	assert.equal(result.code, BUSY_EXIT_CODE);
});

test("the tries fit inside the write budget: the second one gets what is left", async () => {
	const { exec, calls } = fakeExec([BUSY, { code: 0, stdout: "{}", stderr: "" }]);
	const client = createClient({ exec });

	await client.save(NAMED, "/work");

	const first = calls[0].options.timeout;
	const second = calls[1].options.timeout;
	assert.equal(first, WRITE_TIMEOUT_MS, "the first try has the whole budget");
	assert.ok(second < first, `the second try gets what is left: ${second} < ${first}`);
	assert.ok(second > 0, "and it is still a budget, not zero");
});

test("a call that was aborted is not tried again", async () => {
	const { exec, calls } = fakeExec([BUSY]);
	const client = createClient({ exec });
	const controller = new AbortController();

	const pending = client.withSignal(controller.signal).save(NAMED, "/work");
	controller.abort();
	await pending;

	assert.equal(calls.length, 1, `an aborted call is not retried: ${JSON.stringify(calls)}`);
});

// --- R2: the retry is a property of the command -------------------------
//
// The busy exit means the store was locked and nothing was written, and some
// commands are idempotent by construction: a second try of the same call is
// the same effect, never a second record. Those always retry. A `save` is the
// one command whose repeat can create something new, so it retries only when it
// names the record it writes (`namesItsRecord` above). Everything else — a
// read, an init — has no reason to try again and pays one call.

test("the commands that are idempotent by construction are tried again on a locked store", async () => {
	const commands = [
		["sessionStart", (client) => client.sessionStart({ sessionId: "01JX-1", directory: "/work" }, "/work")],
		["sessionEnd", (client) => client.sessionEnd({ sessionId: "01JX-1" }, "/work")],
		["savePrompt", (client) => client.savePrompt({ content: "hola", sessionId: "01JX-1" }, "/work")],
		["bind", (client) => client.bind("payments", "git:key", "/work", "/work")],
		["allowNamespace", (client) => client.allowNamespace("payments", "/work")],
	];
	for (const [name, call] of commands) {
		const { exec, calls } = fakeExec([BUSY, { code: 0, stdout: "{}", stderr: "" }]);
		const client = createClient({ exec });

		const result = await call(client);

		assert.equal(calls.length, 2, `${name}: a locked call is tried again: ${JSON.stringify(calls)}`);
		assert.equal(result.code, 0, `${name}: the second answer is the one that is kept`);
		assert.equal(result.busy_retries, 1, `${name}: the answer says how many tries it took`);
	}
});

test("a locked session end reports the lock once the tries are spent", async () => {
	const { exec, calls } = fakeExec([BUSY, BUSY, BUSY]);
	const client = createClient({ exec });

	const result = await client.sessionEnd({ sessionId: "01JX-1" }, "/work");

	assert.equal(calls.length, 1 + BUSY_RETRY_DELAYS_MS.length, "the tries are still bounded");
	assert.equal(result.code, BUSY_EXIT_CODE, "a store that stays locked is still a failure");
	assert.equal(result.busy_retries, BUSY_RETRY_DELAYS_MS.length);
});

test("an untitled save still does not retry, and a titled one still does", async () => {
	// The two halves of the save rule, side by side: the command's own policy
	// did not take the identity test away with it.
	const untitled = fakeExec([BUSY]);
	const untitledResult = await createClient({ exec: untitled.exec }).save(
		{ namespace: "payments", content: "a fact" },
		"/work",
	);
	assert.equal(untitled.calls.length, 1, "a save with no title would be a second record");
	assert.equal(untitledResult.code, BUSY_EXIT_CODE);

	const titled = fakeExec([BUSY, { code: 0, stdout: "{}", stderr: "" }]);
	const titledResult = await createClient({ exec: titled.exec }).save(NAMED, "/work");
	assert.equal(titled.calls.length, 2, "a save that names its record is the same record again");
	assert.equal(titledResult.code, 0);
});
