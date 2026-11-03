/**
 * The session's children: registered, and killed at shutdown
 * (odd/tasks/ext-ts-rewrite.md, R8).
 *
 * A hook's engine call carries no signal from pi, and the client spawns its
 * child detached, so a child a hook started could outlive the session: it kept
 * writing into a session that was closing and held the store's lock while the
 * next session waited. The registry is the session's handle on them — every
 * call it starts is tracked, the hooks carry its signal, and the shutdown kills
 * what is left before the close spends a millisecond of its budget.
 *
 * Tool calls are untouched: their abort is pi's `ctx.signal`, which already
 * kills the process group of the call it cancels.
 */

import { test } from "node:test";
import assert from "node:assert/strict";
import {
	chmodSync,
	existsSync,
	mkdtempSync,
	readFileSync,
	rmSync,
	writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import mnexisMemory from "../index.ts";
import { createChildRegistry, defaultExec } from "../lib/cli.ts";

/**
 * A fake engine that hangs for a minute, leaves a child of its own behind, and
 * writes its pid where the test can find it.
 */
const HUNG_BINARY = `#!/bin/sh
sleep 60 &
echo $! > "$MNEXIS_TEST_CHILD_PID_FILE"
trap 'printf term > "$MNEXIS_TEST_TERM_FILE"; exit 0' TERM
sleep 60
`;

const ENV_KEYS = [
	"MNEXIS_BIN",
	"MNEXIS_TEST_CHILD_PID_FILE",
	"MNEXIS_TEST_TERM_FILE",
	"MNEXIS_TEST_LOG",
	"MNEXIS_PROMPT_CAPTURE_BUDGET_MS",
	"MNEXIS_SHUTDOWN_BUDGET_MS",
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

/** The hanging engine, installed in a temp directory. */
function hungEngine(t) {
	const dir = mkdtempSync(join(tmpdir(), "mnexis-children-"));
	t.after(() => rmSync(dir, { recursive: true, force: true }));
	const binary = join(dir, "mnexis");
	writeFileSync(binary, HUNG_BINARY);
	chmodSync(binary, 0o755);
	process.env.MNEXIS_BIN = binary;
	process.env.MNEXIS_TEST_CHILD_PID_FILE = join(dir, "child.pid");
	process.env.MNEXIS_TEST_TERM_FILE = join(dir, "term");
	return { dir, binary, childPid: join(dir, "child.pid") };
}

/** Wait until `ready` says so, or give up after `ms`. */
async function waitFor(ready, ms = 5000) {
	const deadline = Date.now() + ms;
	while (Date.now() < deadline) {
		if (ready()) {
			return true;
		}
		await new Promise((resolve) => setTimeout(resolve, 20));
	}
	return false;
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

/** The hooks pi would hand the extension, with a registry under them. */
function host(t) {
	const handlers = new Map();
	const entries = [];
	mnexisMemory({
		registerTool() {},
		on(event, handler) {
			const list = handlers.get(event) ?? [];
			list.push(handler);
			handlers.set(event, list);
		},
		appendEntry(customType, data) {
			entries.push({ customType, data });
		},
	});
	return { handlers, entries };
}

/** The pi context a hook receives. */
function context(dir, sessionId = "01JX-CHILDREN") {
	return {
		cwd: dir,
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

test("the registry kills the child it tracks, and killing twice is safe", async (t) => {
	const restore = snapshotEnv();
	t.after(restore);
	const { binary, childPid } = hungEngine(t);
	const registry = createChildRegistry();

	const pending = defaultExec(binary, ["prompt", "hola"], {
		timeout: 60_000,
		signal: registry.signal,
		registry,
	});
	assert.ok(await waitFor(() => existsSync(childPid) && readFileSync(childPid, "utf8") !== ""), "the child started");
	assert.equal(registry.size, 1, "the registry holds the child while it runs");

	registry.killAll();
	const result = await pending;

	assert.equal(result.cancelled, true, "the pending call is cancelled, not answered");
	assert.equal(registry.size, 0, "and the registry is empty once it is gone");
	const pid = Number.parseInt(readFileSync(childPid, "utf8"), 10);
	assert.ok(await waitFor(() => !alive(pid)), `the engine's own child is gone too: pid ${pid}`);

	// A shutdown that runs twice must be safe: the second kill finds nothing.
	registry.killAll();
	assert.equal(registry.size, 0, "an empty registry stays empty");

	// And a registry killed by one shutdown gives the next session a live one.
	const rearmed = registry.rearm();
	assert.equal(rearmed.signal.aborted, false, "a rearmed registry has a signal that is not spent");
});

test("a hook's child is killed at shutdown, and the hook returns at once", async (t) => {
	const restore = snapshotEnv();
	t.after(restore);
	const { dir, childPid } = hungEngine(t);
	// The capture's own budget is far away on purpose: only the shutdown's
	// kill can end it inside this test.
	process.env.MNEXIS_PROMPT_CAPTURE_BUDGET_MS = "30000";
	process.env.MNEXIS_SHUTDOWN_BUDGET_MS = "300";
	const { handlers, entries } = host(t);

	const capture = emit(handlers, "input", { text: "guardá esto", source: "user" }, context(dir));
	assert.ok(await waitFor(() => existsSync(childPid) && readFileSync(childPid, "utf8") !== ""), "the capture's child started");

	const started = Date.now();
	await emit(handlers, "session_shutdown", { reason: "quit" }, context(dir));
	const elapsed = Date.now() - started;
	assert.ok(elapsed < 2000, `the shutdown does not wait for the capture: ${elapsed} ms`);

	const results = await capture;
	assert.deepEqual(results, [undefined], "the capture returns nothing that changes the turn");
	assert.equal(
		entries.filter((entry) => entry.customType === "mnexis-prompt-capture").length,
		0,
		`a capture killed by the shutdown is expected, not a lost record: ${JSON.stringify(entries)}`,
	);

	const pid = Number.parseInt(readFileSync(childPid, "utf8"), 10);
	assert.ok(await waitFor(() => !alive(pid)), `the capture's child is dead: pid ${pid}`);
});
