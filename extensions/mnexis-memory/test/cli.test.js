import { test } from "node:test";
import assert from "node:assert/strict";
import { createClient, parseJson } from "../lib/cli.js";

/** Fake exec: records calls and replays scripted answers. */
function fakeExec(answers = []) {
	const calls = [];
	const exec = async (command, args) => {
		calls.push({ command, args });
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
