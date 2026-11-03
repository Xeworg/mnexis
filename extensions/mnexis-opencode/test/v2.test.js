/**
 * opencode 2.x: `setup(ctx)` registers hooks on the domains that own them.
 *
 * The same rules, driven through the shape a 2.x host reads — the domains, the
 * catalog transform, the event stream, the cleanup — with the host and the
 * engine both faked. The harness is `helpers/host.js`; the model-facing catalog
 * has its own file.
 */

import assert from "node:assert/strict";
import test from "node:test";

import {
	GRANT_REFUSAL,
	PROFILE_REFUSAL,
	SETTLE_NUDGE,
	SHUTDOWN_BUDGET_ENV,
	createPlugin,
} from "../index.js";
import { argv, callsWith, engine, fakeHost, recordingExec, waitFor } from "./helpers/host.js";

// --- opencode 2.x ---------------------------------------------------------


test("v2: setup registers the four hooks and one subscription, and hands back a cleanup", async () => {
	const { exec } = recordingExec(engine());
	const host = fakeHost();
	const cleanup = await createPlugin({ exec, env: {} }).setup(host.ctx);

	assert.equal(typeof cleanup, "function");
	assert.deepEqual([...host.hooks.keys()].sort(), [
		"session:compaction",
		"session:context",
		"session:prompt",
		"tool:execute.after",
		"tool:execute.before",
	]);
	host.close();
	await cleanup();
});

test("v2: the prompt hook captures the text the host admitted", async () => {
	const { exec, calls } = recordingExec(engine());
	const host = fakeHost();
	const cleanup = await createPlugin({ exec, env: {} }).setup(host.ctx);

	await host.hooks.get("session:prompt")({
		sessionID: "s1",
		messageID: "m1",
		prompt: { text: "haz la extension de opencode de calidad" },
		delivery: "steer",
	});

	assert.equal(argv(callsWith(calls, "session")[0]), "mnexis session --id s1 --directory /repo --json");
	assert.equal(
		argv(callsWith(calls, "prompt")[0]),
		"mnexis prompt haz la extension de opencode de calidad --session s1 --json",
	);

	// Nothing typed is nothing stored.
	await host.hooks.get("session:prompt")({ sessionID: "s1", prompt: { text: "" } });
	assert.equal(callsWith(calls, "prompt").length, 1);

	host.close();
	await cleanup();
});

test("v2: the context hook adds the pointer to the system part, once", async () => {
	const { exec, calls } = recordingExec(engine());
	const host = fakeHost();
	const cleanup = await createPlugin({ exec, env: {} }).setup(host.ctx);

	const event = { sessionID: "s1", system: [], messages: [], tools: {}, options: {} };
	await host.hooks.get("session:context")(event);
	await host.hooks.get("session:context")(event);

	assert.equal(event.system.length, 1);
	assert.equal(event.system[0].type, "text");
	assert.match(event.system[0].text, /namespace `acme`/);
	assert.equal(callsWith(calls, "status").length, 1);

	// A session the host names without an id is not pointed at, and not read.
	const anonymous = { system: [], messages: [], tools: {}, options: {} };
	await host.hooks.get("session:context")(anonymous);
	assert.deepEqual(anonymous.system, []);

	host.close();
	await cleanup();
});

test("v2: the compaction hook keeps the live state", async () => {
	const { exec, calls } = recordingExec(engine());
	const host = fakeHost();
	const cleanup = await createPlugin({ exec, env: {} }).setup(host.ctx);

	const event = { sessionID: "s1", system: [], messages: [], options: {} };
	await host.hooks.get("session:compaction")(event);

	assert.equal(argv(calls[0]), "mnexis list --type project_state,decision --full --limit 8 --json");
	assert.equal(event.system.length, 1);
	assert.match(event.system[0].text, /The door answers previews/);

	host.close();
	await cleanup();
});

test("v2: the gate refuses an escalation before the tool runs", async () => {
	const { exec } = recordingExec(engine());
	const host = fakeHost();
	const cleanup = await createPlugin({ exec, env: {} }).setup(host.ctx);

	const before = host.hooks.get("tool:execute.before");
	assert.throws(() => before({ tool: "shell", input: { command: "mnexis project bind acme --key k --path ." } }), {
		message: GRANT_REFUSAL,
	});
	assert.throws(() => before({ tool: "edit", input: { filePath: "/home/u/.mnexis/projects.toml" } }), {
		message: PROFILE_REFUSAL,
	});
	assert.doesNotThrow(() => before({ tool: "shell", input: { command: "cargo test" } }));
	assert.doesNotThrow(() => before({ tool: "write", input: { filePath: "/repo/NOTES.md" } }));

	host.close();
	await cleanup();
});

test("v2: a session the host creates is registered from the event's own directory", async () => {
	const { exec, calls } = recordingExec(engine());
	const host = fakeHost();
	const cleanup = await createPlugin({ exec, env: {} }).setup(host.ctx);

	host.push({
		type: "session.created",
		data: { sessionID: "s9" },
		location: { directory: "/repo/worktree" },
	});
	assert.ok(await waitFor(() => callsWith(calls, "session").length === 1), "the session was registered");
	assert.equal(argv(callsWith(calls, "session")[0]), "mnexis session --id s9 --directory /repo/worktree --json");

	host.close();
	await cleanup();
});

test("v2: a host missing one seat still gets the other rules", async () => {
	const { exec } = recordingExec(engine());
	const host = fakeHost();
	// An older 2.x without a compaction seat, and a host whose tool hooks are
	// not available: the capture, above all, must still be registered.
	host.ctx.session.hook = async (name, callback) => {
		if (name === "compaction") {
			throw new Error("no such hook");
		}
		host.hooks.set(`session:${name}`, callback);
		return { dispose: async () => {} };
	};
	host.ctx.tool.hook = async () => {
		throw new Error("no tool hooks");
	};
	// ... and a host whose catalog the plugin may not rewrite at all.
	host.ctx.tool.transform = async () => {
		throw new Error("no catalog");
	};

	const cleanup = await createPlugin({ exec, env: {} }).setup(host.ctx);

	assert.equal(typeof cleanup, "function");
	assert.ok(host.hooks.has("session:prompt"));
	assert.ok(host.hooks.has("session:context"));
	assert.equal(host.hooks.has("session:compaction"), false);
	assert.equal(host.hooks.has("tool:execute.before"), false);
	assert.equal(host.transforms.length, 0);

	host.close();
	await cleanup();
});

test("v2: the cleanup kills the calls still in flight and never ends the session", async () => {
	const events = [];
	const registry = { killAll: () => events.push("kill") };
	const { exec, calls } = recordingExec((args) => {
		events.push(args[0]);
		return engine()(args);
	});
	const host = fakeHost();
	const cleanup = await createPlugin({ exec, env: {}, registry }).setup(host.ctx);

	await host.hooks.get("session:prompt")({ sessionID: "s1", prompt: { text: "hola" } });
	host.close();
	const report = await cleanup();

	assert.deepEqual(events, ["session", "prompt", "kill"], "the children die first, and nothing follows");
	assert.deepEqual(callsWith(calls, "end"), [], "a reload replaces the plugin, not the session");
	assert.deepEqual(callsWith(calls, "save"), []);
	assert.equal(report.action, "skipped");
});

test("v2: a workspace with no namespace is pointed at nothing, and the read is not repeated", async () => {
	const { exec, calls } = recordingExec(
		engine({ status: { stdout: JSON.stringify({ workspace: {}, memory_count: 0 }) } }),
	);
	const host = fakeHost();
	const cleanup = await createPlugin({ exec, env: {} }).setup(host.ctx);

	const event = { sessionID: "s1", system: [], messages: [], options: {} };
	await host.hooks.get("session:context")(event);
	await host.hooks.get("session:context")(event);

	assert.deepEqual(event.system, []);
	assert.equal(callsWith(calls, "status").length, 1);

	host.close();
	await cleanup();
});

// The settle question rides V2's context seat: the same ask, once, decided on
// the session's own outcomes. The rationale is beside the v1 tests.

test("v2: the settle question rides the context seat once, from the session's own outcomes", async () => {
	const { exec } = recordingExec(engine());
	const host = fakeHost();
	const cleanup = await createPlugin({ exec, env: {} }).setup(host.ctx);

	const after = host.hooks.get("tool:execute.after");
	assert.equal(typeof after, "function", "the outcome seat is registered beside the gate");
	assert.doesNotThrow(() => after(undefined), "an outcome the plugin cannot read never breaks the turn");

	const event = { sessionID: "s1", system: [], messages: [], options: {} };
	await host.hooks.get("session:context")(event);
	after({ tool: "shell", sessionID: "s1", input: { command: "cargo test" } });
	after({ tool: "mnexis_memory_save", sessionID: "s1", isError: true });
	await host.hooks.get("session:context")(event);
	await host.hooks.get("session:context")(event);

	assert.equal(event.system.filter((part) => part.text === SETTLE_NUDGE).length, 1);

	// A session whose save landed is never asked.
	const quiet = { sessionID: "s2", system: [], messages: [], options: {} };
	after({ tool: "edit", sessionID: "s2", input: { filePath: "/repo/src/main.rs" } });
	after({ tool: "mnexis_memory_save", sessionID: "s2" });
	await host.hooks.get("session:context")(quiet);
	assert.equal(quiet.system.some((part) => part.text === SETTLE_NUDGE), false);

	host.close();
	await cleanup();
});

// The lifecycle on V2: the event stream registers and closes, and the cleanup
// repeats neither. The engine's semantics are beside the v1 tests.

test("v2: the host's session.deleted ends that session, and the cleanup does not repeat it", async () => {
	const { exec, calls } = recordingExec(engine());
	const host = fakeHost();
	const cleanup = await createPlugin({ exec, env: {} }).setup(host.ctx);

	host.push({ type: "session.created", data: { sessionID: "s7" } });
	assert.ok(await waitFor(() => callsWith(calls, "session").length === 1), "the session was registered");
	host.push({ type: "session.deleted", data: { sessionID: "s7" } });
	assert.ok(await waitFor(() => callsWith(calls, "end").length === 1), "the deleted session was ended");

	host.close();
	await cleanup();

	assert.deepEqual(callsWith(calls, "end").map(argv), ["mnexis end --id s7 --json"]);
});

// The close on V2's own end signal: `end --json` answers the namespace, and the
// record goes through the same write path v1's close takes.

test("v2: the host's session.deleted writes the estado al cierre of that session", async () => {
	const { exec, calls } = recordingExec(engine());
	const host = fakeHost();
	const cleanup = await createPlugin({ exec, env: {} }).setup(host.ctx);

	host.push({ type: "session.created", data: { sessionID: "s7" } });
	assert.ok(await waitFor(() => callsWith(calls, "session").length === 1), "the session was registered");
	host.push({ type: "session.deleted", data: { sessionID: "s7" } });
	assert.ok(await waitFor(() => callsWith(calls, "save").length === 1), "the closing state was written");

	const save = callsWith(calls, "save")[0];
	assert.deepEqual(save.args.slice(0, 7), [
		"save",
		"--namespace",
		"acme",
		"--type",
		"project_state",
		"--title",
		"estado al cierre",
	]);
	assert.match(save.args[save.args.length - 2], /Session s7 ended/);

	host.close();
	await cleanup();
});

// --- the shutdown budget (fix 3) -----------------------------------------
//
// pi awaits its shutdown with no timeout, so it gives the close one of its own
// (R1, `lib/budget.ts`). Here the close is the work a host hook awaits, and it
// gets the same one budget for all of its steps; a step that does not fit is
// abandoned, journaled, and the session stays tracked so the next close signal
// can retry it.

test("v2: a close that hangs is abandoned inside the budget, and the host returns", async () => {
	const host = fakeHost();
	const { exec, calls } = recordingExec((args) => {
		if (args[0] === "end") return new Promise(() => {});
		return engine()(args);
	});
	const cleanup = await createPlugin({ exec, env: { [SHUTDOWN_BUDGET_ENV]: "60" } }).setup(host.ctx);

	host.push({ type: "session.created", data: { sessionID: "s9" } });
	assert.ok(await waitFor(() => callsWith(calls, "session").length === 1), "the session was registered");
	const started = Date.now();
	host.push({ type: "session.deleted", data: { sessionID: "s9" } });
	assert.ok(
		await waitFor(() => host.journal.has("mnexis.report.session-end"), { timeout: 2000 }),
		"the abandonment reached the journal this host has",
	);
	const elapsed = Date.now() - started;

	assert.ok(elapsed < 2000, `the close returned inside its budget (${elapsed} ms)`);
	const record = host.journal.get("mnexis.report.session-end");
	assert.equal(record.action, "abandoned");
	assert.match(record.reason, /end did not finish within the 60 ms shutdown budget/);
	assert.equal(record.session, "s9");
	assert.equal(callsWith(calls, "save").length, 0, "nothing is written after an abandoned end");

	host.close();
	await cleanup();
});

test("v2: a refusal the host cannot log reaches the journal it has", async () => {
	const host = fakeHost();
	const { exec } = recordingExec(engine({ prompt: { code: 4, stderr: "forbidden" } }));
	const cleanup = await createPlugin({ exec, env: {} }).setup(host.ctx);

	await host.hooks.get("session:prompt")({ sessionID: "s1", prompt: { text: "hola" } });

	assert.ok(await waitFor(() => host.journal.has("mnexis.report.capture")), "the refusal was journaled");
	const record = host.journal.get("mnexis.report.capture");
	assert.equal(record.exit_code, 4);
	assert.equal(JSON.stringify(record).includes("hola"), false, "the text never reaches the journal");

	host.close();
	await cleanup();
});
