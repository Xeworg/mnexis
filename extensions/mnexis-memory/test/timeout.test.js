/**
 * S2 (audit A9): a hung engine can no longer hang the session.
 *
 * The audit measured it: "signal is accepted by all 14 execute signatures and
 * never used; a hung mnexis blocks session_start, every input hook and every
 * save, and pi's abort cannot kill the child". Every test here drives a real
 * tool path against a real child process, so the evidence is the call
 * returning: an engine that never answers is a process the extension has to
 * kill, and the fake engine leaves a grandchild behind to prove the kill
 * reaches the whole process group and not just the process it spawned.
 *
 * The budgets are constants of the extension (`lib/cli.js`); MNEXIS_TIMEOUT_MS
 * shrinks them so the suite pays a fifth of a second for a conclusion it would
 * otherwise wait ten seconds for.
 */

import { test } from "node:test";
import assert from "node:assert/strict";
import { chmodSync, existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import mnexisMemory from "../index.js";

/**
 * A fake engine that answers nothing until its sleep is over, and leaves a
 * child of its own behind: the shape of the hang the audit measured, where a
 * kill aimed at the direct child leaves the work running.
 *
 * It is a shell script so it starts in milliseconds: the test gives the
 * extension 200 ms, and a fake that needed a second to boot would be a fake
 * that measures node's startup instead of the kill.
 */
const HUNG_BINARY = `#!/bin/sh
if [ "${'${MNEXIS_TEST_HANG_SECONDS:-0}'}" = "0" ]; then
	printf '{"count":0,"items":[]}'
	exit 0
fi
sleep "$MNEXIS_TEST_HANG_SECONDS" &
echo $! > "$MNEXIS_TEST_CHILD_PID_FILE"
trap 'printf term > "$MNEXIS_TEST_TERM_FILE"; exit 0' TERM
sleep "$MNEXIS_TEST_HANG_SECONDS"
`;

const ENV_KEYS = [
	"MNEXIS_BIN",
	"MNEXIS_TIMEOUT_MS",
	"MNEXIS_TEST_HANG_SECONDS",
	"MNEXIS_TEST_TERM_FILE",
	"MNEXIS_TEST_CHILD_PID_FILE",
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
	const dir = mkdtempSync(join(tmpdir(), "mnexis-timeout-"));
	const binary = join(dir, "mnexis");
	writeFileSync(binary, HUNG_BINARY);
	chmodSync(binary, 0o755);
	process.env.MNEXIS_BIN = binary;
	process.env.MNEXIS_TEST_TERM_FILE = join(dir, "term");
	process.env.MNEXIS_TEST_CHILD_PID_FILE = join(dir, "child.pid");
	return {
		dir,
		term: join(dir, "term"),
		childPid: join(dir, "child.pid"),
	};
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

/** Whether a process is still running. */
function alive(pid) {
	try {
		process.kill(pid, 0);
		return true;
	} catch {
		return false;
	}
}

/** Wait for a file to appear, so a kill that has not landed yet is not a pass. */
async function waitForFile(path, timeoutMs = 3000) {
	const deadline = Date.now() + timeoutMs;
	while (Date.now() < deadline) {
		if (existsSync(path)) return true;
		await new Promise((resolve) => setTimeout(resolve, 20));
	}
	return existsSync(path);
}

/** Wait for the engine's own child to die, and make sure it does not outlive the test. */
async function waitForDeath(pid, timeoutMs = 3000) {
	const deadline = Date.now() + timeoutMs;
	while (Date.now() < deadline) {
		if (!alive(pid)) return true;
		await new Promise((resolve) => setTimeout(resolve, 20));
	}
	const dead = !alive(pid);
	if (!dead) {
		try {
			process.kill(pid, "SIGKILL");
		} catch {
			// Already gone between the check and the cleanup.
		}
	}
	return dead;
}

test("a hung engine is killed at the timeout: the call fails fast, naming the command and the budget", { timeout: 15_000 }, async () => {
	const restore = snapshotEnv();
	const scratch = setup();
	process.env.MNEXIS_TIMEOUT_MS = "200";
	process.env.MNEXIS_TEST_HANG_SECONDS = "20";
	try {
		const { tools, pi } = fakeHost();
		mnexisMemory(pi);
		const started = Date.now();
		const result = await tools
			.get("mnexis_search")
			.execute("t1", { query: "puerto" }, undefined, undefined, context());
		const elapsed = Date.now() - started;

		assert.equal(result.isError, true, "a call the engine never answered is a failed call");
		const text = textOf(result);
		assert.match(text, /mnexis search/, `the answer names the command: ${text}`);
		assert.match(text, /did not answer within 0\.2 s/, `the answer names the budget: ${text}`);
		assert.ok(elapsed < 5000, `the call returned instead of hanging: ${elapsed} ms`);
		assert.ok(
			await waitForFile(scratch.term),
			`the engine was killed, not abandoned: ${scratch.term}`,
		);
		assert.ok(
			await waitForFile(scratch.childPid),
			`the engine named the child it started: ${scratch.childPid}`,
		);
		const pid = Number(readFileSync(scratch.childPid, "utf8").trim());
		assert.ok(pid > 0, `the engine named the child it left behind: ${pid}`);
		assert.ok(
			await waitForDeath(pid),
			`the kill reached everything the engine started: pid ${pid} is still running`,
		);
	} finally {
		restore();
		rmSync(scratch.dir, { recursive: true, force: true });
	}
});

test("pi's abort kills the child instead of waiting for the budget", { timeout: 15_000 }, async () => {
	const restore = snapshotEnv();
	const scratch = setup();
	// Only the abort can end this call: the budget is far past the test.
	process.env.MNEXIS_TIMEOUT_MS = "60000";
	process.env.MNEXIS_TEST_HANG_SECONDS = "20";
	try {
		const { tools, pi } = fakeHost();
		mnexisMemory(pi);
		const controller = new AbortController();
		const started = Date.now();
		const timer = setTimeout(() => controller.abort(), 150);
		const result = await tools
			.get("mnexis_search")
			.execute("t2", { query: "puerto" }, controller.signal, undefined, context());
		clearTimeout(timer);
		const elapsed = Date.now() - started;

		assert.equal(result.isError, true, "a cancelled call is a failed call");
		const text = textOf(result);
		assert.match(text, /mnexis search/, `the answer names the command: ${text}`);
		assert.match(text, /cancelled/i, `the answer says it was cancelled: ${text}`);
		assert.ok(elapsed < 5000, `the abort ended the call instead of waiting: ${elapsed} ms`);
		assert.ok(
			await waitForFile(scratch.term),
			`the engine was killed, not abandoned: ${scratch.term}`,
		);
	} finally {
		restore();
		rmSync(scratch.dir, { recursive: true, force: true });
	}
});

test("an engine that answers still answers: the budget costs the working path nothing", { timeout: 15_000 }, async () => {
	const restore = snapshotEnv();
	const scratch = setup();
	process.env.MNEXIS_TIMEOUT_MS = "5000";
	process.env.MNEXIS_TEST_HANG_SECONDS = "0";
	try {
		const { tools, pi } = fakeHost();
		mnexisMemory(pi);
		const result = await tools
			.get("mnexis_search")
			.execute("t3", { query: "puerto" }, undefined, undefined, context());

		assert.notEqual(result.isError, true);
		assert.match(textOf(result), /No memories matched 'puerto'/);
	} finally {
		restore();
		rmSync(scratch.dir, { recursive: true, force: true });
	}
});

test("a hung engine cannot block a save either: the workspace resolution is a call with a budget", { timeout: 15_000 }, async () => {
	const restore = snapshotEnv();
	const scratch = setup();
	process.env.MNEXIS_TIMEOUT_MS = "200";
	process.env.MNEXIS_TEST_HANG_SECONDS = "20";
	try {
		const { tools, pi } = fakeHost();
		mnexisMemory(pi);
		const started = Date.now();
		const result = await tools
			.get("mnexis_save")
			.execute("t4", { content: "el puerto es 8081" }, undefined, undefined, context());
		const elapsed = Date.now() - started;

		assert.equal(result.isError, true, "a save whose engine never answered is a failed save");
		const text = textOf(result);
		assert.match(text, /mnexis project list/, `the answer names the call it gave up on: ${text}`);
		assert.match(text, /did not answer within 0\.2 s/, `the answer names the budget: ${text}`);
		assert.match(text, /No memory was written/, `the answer says nothing was written: ${text}`);
		assert.ok(elapsed < 5000, `the save returned instead of hanging: ${elapsed} ms`);
	} finally {
		restore();
		rmSync(scratch.dir, { recursive: true, force: true });
	}
});
