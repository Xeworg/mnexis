/**
 * The evidence of a changed workspace (odd/tasks/nudge-enriched.md, S3).
 *
 * The settle nudge asks the model once whether the session deserved a
 * memory, and it only asks when the session's own events show unsaved work.
 * That evidence was the tool names alone: a successful `write` or `edit`.
 * But a session that changes files through `bash` — `chmod`, `sed -i`, a
 * redirect — showed the extension nothing, and the nudge stayed silent
 * about real work. The checklist bench's pilot measured it: **6 of 41
 * sessions** wrote files through bash and were invisible to the gate,
 * including the pilot's only session that saved nothing.
 *
 * So the gate also asks the workspace itself: the state git sees at the
 * session's start, against the state it sees at the settle. These tests use
 * a real repository in a temporary directory — the mechanism is git's, and a
 * fake would only prove the fake.
 */

import { test } from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import mnexisMemory from "../index.ts";

/** A host that records the handlers, with no engine under the hooks. */
function fakeHost(t) {
	const dir = mkdtempSync(join(tmpdir(), "mnexis-workspace-"));
	t.after(() => rmSync(dir, { recursive: true, force: true }));
	const previous = process.env.MNEXIS_BIN;
	// The session hooks call the engine; a missing binary is the quietest
	// way to keep the corpus hermetic.
	process.env.MNEXIS_BIN = join(dir, "missing-mnexis");
	t.after(() => {
		if (previous === undefined) {
			delete process.env.MNEXIS_BIN;
		} else {
			process.env.MNEXIS_BIN = previous;
		}
	});

	const handlers = new Map();
	mnexisMemory({
		registerTool() {},
		on(event, handler) {
			const list = handlers.get(event) ?? [];
			list.push(handler);
			handlers.set(event, list);
		},
		appendEntry() {},
	});
	return { handlers };
}

/** A workspace that is a git repository with one committed file. */
function repository(t) {
	const dir = mkdtempSync(join(tmpdir(), "mnexis-repo-"));
	t.after(() => rmSync(dir, { recursive: true, force: true }));
	const git = (...args) => execFileSync("git", args, { cwd: dir, stdio: "ignore" });
	git("init", "-q");
	git("config", "user.email", "bench@example.internal");
	git("config", "user.name", "bench");
	writeFileSync(join(dir, "script.sh"), "#!/bin/sh\necho one\n");
	git("add", "-A");
	git("commit", "-qm", "fixture");
	return dir;
}

/** A workspace that is not a repository at all. */
function plainDirectory(t) {
	const dir = mkdtempSync(join(tmpdir(), "mnexis-plain-"));
	t.after(() => rmSync(dir, { recursive: true, force: true }));
	return dir;
}

/** The pi context of a session, in the given workspace. */
function context(sessionId, cwd) {
	return {
		cwd,
		hasUI: false,
		mode: "print",
		ui: { confirm: async () => false },
		sessionManager: { getSessionId: () => sessionId },
	};
}

/** Fire one event through the handlers the extension registered for it. */
async function emit(handlers, event, payload, ctx) {
	const results = [];
	for (const handler of handlers.get(event) ?? []) {
		results.push(await handler({ type: event, ...payload }, ctx));
	}
	return results;
}

/** The nudge's answer: the entry it would append, or nothing. */
function nudgeOf(results) {
	const result = results[0];
	if (!result) {
		return null;
	}
	return result.entries?.[0]?.customType ?? null;
}

test("a session that changed a file through bash is asked once", async (t) => {
	const { handlers } = fakeHost(t);
	const cwd = repository(t);
	await emit(handlers, "session_start", {}, context("01JX", cwd));

	// The model changed a file without the extension seeing a write or an
	// edit: the tool names alone would say nothing happened.
	writeFileSync(join(cwd, "script.sh"), "#!/bin/sh\necho two\n");

	const results = await emit(handlers, "agent_before_settle", {}, context("01JX", cwd));
	assert.equal(
		nudgeOf(results),
		"mnexis-settle-nudge",
		`the workspace's own state is evidence of work: ${JSON.stringify(results)}`,
	);
});

test("a session that changed nothing is not asked", async (t) => {
	const { handlers } = fakeHost(t);
	const cwd = repository(t);
	await emit(handlers, "session_start", {}, context("01JX", cwd));

	const results = await emit(handlers, "agent_before_settle", {}, context("01JX", cwd));
	assert.equal(nudgeOf(results), null, `a session that changed nothing pays nothing: ${JSON.stringify(results)}`);
});

test("a workspace that was already dirty before the session is not work", async (t) => {
	const { handlers } = fakeHost(t);
	const cwd = repository(t);
	// The tree was already dirty when the session started: the session did
	// not make it dirty, and the question is about the session's own work.
	writeFileSync(join(cwd, "script.sh"), "#!/bin/sh\necho dirty\n");
	await emit(handlers, "session_start", {}, context("01JX", cwd));

	const results = await emit(handlers, "agent_before_settle", {}, context("01JX", cwd));
	assert.equal(nudgeOf(results), null, `the dirt was there before: ${JSON.stringify(results)}`);
});

test("a workspace that is not a repository keeps the tool's own evidence", async (t) => {
	const { handlers } = fakeHost(t);
	const cwd = plainDirectory(t);
	await emit(handlers, "session_start", {}, context("01JX", cwd));

	await emit(
		handlers,
		"tool_execution_end",
		{ toolCallId: "w-1", toolName: "write", isError: false, result: {} },
		context("01JX", cwd),
	);

	const results = await emit(handlers, "agent_before_settle", {}, context("01JX", cwd));
	assert.equal(
		nudgeOf(results),
		"mnexis-settle-nudge",
		`without git, the tool that wrote is still evidence: ${JSON.stringify(results)}`,
	);
});

test("a session that already saved is not asked, however the workspace looks", async (t) => {
	const { handlers } = fakeHost(t);
	const cwd = repository(t);
	await emit(handlers, "session_start", {}, context("01JX", cwd));
	writeFileSync(join(cwd, "script.sh"), "#!/bin/sh\necho two\n");
	await emit(
		handlers,
		"tool_call",
		{ toolCallId: "s-1", toolName: "mnexis_save", input: {} },
		context("01JX", cwd),
	);

	const results = await emit(handlers, "agent_before_settle", {}, context("01JX", cwd));
	assert.equal(nudgeOf(results), null, `a session that judged already paid: ${JSON.stringify(results)}`);
});

test("the question is asked once even when the workspace changed", async (t) => {
	const { handlers } = fakeHost(t);
	const cwd = repository(t);
	await emit(handlers, "session_start", {}, context("01JX", cwd));
	writeFileSync(join(cwd, "script.sh"), "#!/bin/sh\necho two\n");

	await emit(handlers, "agent_before_settle", {}, context("01JX", cwd));
	writeFileSync(join(cwd, "script.sh"), "#!/bin/sh\necho three\n");
	const second = await emit(handlers, "agent_before_settle", {}, context("01JX", cwd));

	assert.equal(nudgeOf(second), null, `the question is asked once: ${JSON.stringify(second)}`);
});
