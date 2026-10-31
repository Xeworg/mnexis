import { test } from "node:test";
import assert from "node:assert/strict";
import { READ_TIMEOUT_MS, WRITE_TIMEOUT_MS, createClient, parseJson } from "../lib/cli.js";

/** Fake exec: records calls and replays scripted answers. */
function fakeExec(answers = []) {
	const calls = [];
	const exec = async (command, args, options) => {
		calls.push({ command, args, options });
		const answer = answers.shift();
		if (!answer) {
			return { code: 0, stdout: "", stderr: "" };
		}
		return answer;
	};
	return { exec, calls };
}

test("parseJson tolerates empty output and noisy prefixes", () => {
	assert.equal(parseJson(""), null);
	assert.equal(parseJson("not json"), null);
	assert.deepEqual(parseJson('{"a":1}'), { a: 1 });
	assert.deepEqual(parseJson('noise before {"a":1} noise after'), { a: 1 });
});

test("the client adds --json exactly where it is needed", async () => {
	const { exec, calls } = fakeExec([
		{ code: 0, stdout: '{"bindings":[]}', stderr: "" },
	]);
	const client = createClient({ exec, binary: "mnexis" });
	await client.projectList("/w");
	assert.deepEqual(calls[0].args, ["project", "list", "--json"]);
});

test("save forwards content, tags, topic key and idempotency key", async () => {
	const { exec, calls } = fakeExec([{ code: 0, stdout: '{"uuid":"u1"}', stderr: "" }]);
	const client = createClient({ exec, binary: "mnexis" });
	await client.save(
		{ namespace: "aegispi", content: "a fact", tags: "x,y", topicKey: "t", idempotencyKey: "i" },
		"/w",
	);
	assert.deepEqual(calls[0].args, [
		"save",
		"--namespace",
		"aegispi",
		"--tags",
		"x,y",
		"--topic-key",
		"t",
		"--idempotency-key",
		"i",
		"a fact",
		"--json",
	]);
});

test("search, list, show and supersede use the documented flags", async () => {
	const { exec, calls } = fakeExec([
		{ code: 0, stdout: "[]", stderr: "" },
		{ code: 0, stdout: "[]", stderr: "" },
		{ code: 0, stdout: "{}", stderr: "" },
		{ code: 0, stdout: "", stderr: "" },
	]);
	const client = createClient({ exec, binary: "mnexis" });
	await client.search({ query: "q", namespace: "ns", limit: 3 }, "/w");
	await client.list({ namespace: "ns" }, "/w");
	await client.show("uuid-1", "/w");
	await client.supersede({ older: "a", newer: "b", reason: "why" }, "/w");

	assert.deepEqual(calls[0].args, ["search", "q", "--namespace", "ns", "--limit", "3", "--json"]);
	assert.deepEqual(calls[1].args, ["list", "--namespace", "ns", "--json"]);
	assert.deepEqual(calls[2].args, ["show", "uuid-1", "--json"]);
	assert.deepEqual(calls[3].args, ["supersede", "--older", "a", "--newer", "b", "--reason", "why"]);
});

test("list carries the value filter and the offset that pages the matches", async () => {
	const { exec, calls } = fakeExec([{ code: 0, stdout: '{"items":[]}', stderr: "" }]);
	const client = createClient({ exec, binary: "mnexis" });
	await client.list(
		{ namespace: "ns", value: "8081", limit: 100, offset: 200, full: true },
		"/w",
	);
	assert.deepEqual(calls[0].args, [
		"list",
		"--namespace",
		"ns",
		"--limit",
		"100",
		"--offset",
		"200",
		"--full",
		"--value",
		"8081",
		"--json",
	]);
});

test("the binary can be overridden with MNEXIS_BIN", async () => {
	const { exec, calls } = fakeExec([{ code: 0, stdout: "", stderr: "" }]);
	const client = createClient({ exec, env: { MNEXIS_BIN: "/opt/mnexis" } });
	await client.init("/w");
	assert.equal(calls[0].command, "/opt/mnexis");
});

test("a failing command resolves with its exit code and stderr", async () => {
	const { exec } = fakeExec([{ code: 4, stdout: "", stderr: "forbidden" }]);
	const client = createClient({ exec, binary: "mnexis" });
	const result = await client.allowNamespace("aegispi", "/w");
	assert.equal(result.code, 4);
	assert.equal(result.stderr, "forbidden");
});

test("every call carries a timeout: the read budget for a read, the write budget for a write", async () => {
	// S2 (audit A9): no call may run without a bound. The two budgets are the
	// extension's, and they follow the shape of the call: a read is one query,
	// a write may wait on the lock and pay for its fsync.
	const { exec, calls } = fakeExec([
		{ code: 0, stdout: "{}", stderr: "" },
		{ code: 0, stdout: "{}", stderr: "" },
		{ code: 0, stdout: "{}", stderr: "" },
	]);
	const client = createClient({ exec, binary: "mnexis", env: {} });
	await client.search({ query: "q" }, "/w");
	await client.save({ namespace: "ns", content: "c" }, "/w");
	await client.allowNamespace("ns", "/w");

	assert.equal(calls[0].options.timeout, READ_TIMEOUT_MS);
	assert.equal(calls[1].options.timeout, WRITE_TIMEOUT_MS);
	assert.equal(calls[2].options.timeout, WRITE_TIMEOUT_MS, "granting a namespace writes the profile");
});

test("MNEXIS_TIMEOUT_MS overrides both budgets", async () => {
	const { exec, calls } = fakeExec([
		{ code: 0, stdout: "{}", stderr: "" },
		{ code: 0, stdout: "{}", stderr: "" },
	]);
	const client = createClient({ exec, binary: "mnexis", env: { MNEXIS_TIMEOUT_MS: "1500" } });
	await client.search({ query: "q" }, "/w");
	await client.save({ namespace: "ns", content: "c" }, "/w");

	assert.equal(calls[0].options.timeout, 1500);
	assert.equal(calls[1].options.timeout, 1500);
});

test("the call a client made is named in its result, and the signal is handed to exec", async () => {
	const { exec, calls } = fakeExec([
		{ code: 0, stdout: '{"count":0,"items":[]}', stderr: "" },
		{ code: 0, stdout: '{"bindings":[]}', stderr: "" },
	]);
	const controller = new AbortController();
	const client = createClient({ exec, binary: "mnexis", env: {} });
	const answered = await client.withSignal(controller.signal).search({ query: "q" }, "/w");
	const resolved = await client.projectList("/w");

	assert.equal(calls[0].options.signal, controller.signal);
	assert.equal(calls[1].options.signal, null, "a call with no signal carries none");
	assert.equal(answered.command, "search");
	assert.equal(resolved.command, "project list");
	assert.equal(answered.timeout_ms, READ_TIMEOUT_MS);
});

test("a JSON call that got no answer says so instead of answering null", async () => {
	// S3 (audit A8): `parseJson` says null for silence, and every caller used to
	// read that null as a fact. The client is the one place that knows the
	// difference between an answer and the absence of one.
	const { exec } = fakeExec([
		{ code: 0, stdout: "", stderr: "" },
		{ code: 0, stdout: "mnexis: the store is locked", stderr: "" },
		{ code: 0, stdout: '{"count":0,"items":[]}', stderr: "" },
		{ code: 0, stdout: "", stderr: "" },
	]);
	const client = createClient({ exec, binary: "mnexis", env: {} });

	const silent = await client.search({ query: "q" }, "/w");
	const unreadable = await client.list({}, "/w");
	const answered = await client.search({ query: "q" }, "/w");
	const textCommand = await client.supersede({ older: "a", newer: "b", reason: "r" }, "/w");

	assert.equal(silent.data, null);
	assert.equal(silent.no_answer, "empty");
	assert.equal(unreadable.no_answer, "unreadable");
	assert.equal(answered.no_answer, null, "an answer with no items is still an answer");
	assert.equal(textCommand.no_answer, null, "a command whose answer is not JSON has nothing to read");
});
