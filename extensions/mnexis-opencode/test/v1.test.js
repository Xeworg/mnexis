/**
 * opencode 1.x: the plugin function returns one hooks object.
 *
 * The plugin's rules, driven through the shape a 1.x host reads — the hooks
 * object, its events, its unload — with the engine faked so the exact CLI argv
 * is visible. The harness is `helpers/host.js`.
 */

import assert from "node:assert/strict";
import test from "node:test";

import plugin, {
	GRANT_REFUSAL,
	PLUGIN_ID,
	POINTER_ENV,
	PROFILE_REFUSAL,
	SETTLE_NUDGE,
	SHUTDOWN_BUDGET_ENV,
	createPlugin,
} from "../index.js";
import { argv, callsWith, engine, recordingExec } from "./helpers/host.js";

// --- the shape both hosts read --------------------------------------------

test("the default export is the object both plugin models load", () => {
	// opencode 1.x: detect-mode needs an `id` and calls `.server(input, options)`.
	// opencode 2.x: decodes `{ id, setup }` and runs `setup(ctx)`.
	assert.equal(typeof plugin, "object");
	assert.equal(plugin.id, PLUGIN_ID);
	assert.equal(typeof plugin.server, "function");
	assert.equal(typeof plugin.setup, "function");
});

// --- opencode 1.x ---------------------------------------------------------

test("v1: what the human typed is stored verbatim, in the session it was said in", async () => {
	const { exec, calls } = recordingExec(engine());
	const hooks = await createPlugin({ exec, env: {} }).server({ directory: "/repo" });

	await hooks["chat.message"](
		{ sessionID: "s1" },
		{
			message: { role: "user" },
			parts: [
				{ type: "text", text: "es importante hacer que la extension sea de calidad" },
				{ type: "file", mime: "image/png", url: "data:image/png;base64,AA" },
			],
		},
	);

	assert.equal(argv(callsWith(calls, "session")[0]), "mnexis session --id s1 --directory /repo --json");
	assert.equal(
		argv(callsWith(calls, "prompt")[0]),
		"mnexis prompt es importante hacer que la extension sea de calidad --session s1 --json",
	);
	// The session exists before the prompt that joins it.
	assert.ok(
		calls.findIndex((call) => call.args[0] === "session") <
			calls.findIndex((call) => call.args[0] === "prompt"),
	);
});

test("v1: a message the host wrote is not captured", async () => {
	const { exec, calls } = recordingExec(engine());
	const hooks = await createPlugin({ exec, env: {} }).server({ directory: "/repo" });

	await hooks["chat.message"](
		{ sessionID: "s1" },
		{ message: { role: "user" }, parts: [{ type: "text", text: "continue", synthetic: true }] },
	);
	await hooks["chat.message"]({ sessionID: "s1" }, { message: { role: "assistant" }, parts: [] });

	assert.deepEqual(calls, []);
});

test("v1: a session the host opens is registered, in the directory the host names", async () => {
	const { exec, calls } = recordingExec(engine());
	const hooks = await createPlugin({ exec, env: {} }).server({ directory: "/repo" });

	await hooks.event({
		event: { type: "session.created", properties: { info: { id: "s7", directory: "/repo/sub" } } },
	});
	await hooks.event({ event: { type: "session.idle", properties: { sessionID: "s7" } } });
	await hooks.event({ event: { type: "message.updated", properties: {} } });

	assert.equal(calls.length, 1);
	assert.equal(argv(calls[0]), "mnexis session --id s7 --directory /repo/sub --json");
});

test("v1: the host's worktree is the directory when it names no directory", async () => {
	const { exec, calls } = recordingExec(engine());
	const hooks = await createPlugin({ exec, env: {} }).server({ worktree: "/wt" });

	await hooks.event({ event: { type: "session.created", properties: { info: { id: "s1" } } } });

	assert.equal(argv(calls[0]), "mnexis session --id s1 --directory /wt --json");
});

test("v1: the gate refuses a model-issued grant and lets the rest through", async () => {
	const { exec } = recordingExec(engine());
	const hooks = await createPlugin({ exec, env: {} }).server({ directory: "/repo" });

	await assert.rejects(
		() => hooks["tool.execute.before"]({ tool: "bash" }, { args: { command: "mnexis config allow-namespace acme" } }),
		(error) => {
			assert.equal(error.message, GRANT_REFUSAL);
			return true;
		},
	);
	await assert.rejects(
		() => hooks["tool.execute.before"]({ tool: "write" }, { args: { filePath: "/home/u/.mnexis/mnexis.toml" } }),
		(error) => {
			assert.equal(error.message, PROFILE_REFUSAL);
			return true;
		},
	);
	// The model's own work is untouched.
	await hooks["tool.execute.before"]({ tool: "bash" }, { args: { command: "mnexis context 'the port'" } });
	await hooks["tool.execute.before"]({ tool: "write" }, { args: { filePath: "/repo/src/main.rs" } });
});

test("v1: the pointer is delivered once per session, from the engine's own answer", async () => {
	const { exec, calls } = recordingExec(engine());
	const hooks = await createPlugin({ exec, env: {} }).server({ directory: "/repo" });

	const system = [];
	await hooks["experimental.chat.system.transform"]({ sessionID: "s1" }, { system });
	await hooks["experimental.chat.system.transform"]({ sessionID: "s1" }, { system });

	assert.equal(system.length, 1);
	assert.match(system[0], /namespace `acme`/);
	assert.match(system[0], /mnexis_memory_context/);
	assert.equal(callsWith(calls, "status").length, 1);
	assert.equal(callsWith(calls, "recall", "--previous").length, 1);

	// A second session is a second answer: the previous session changed.
	const other = [];
	await hooks["experimental.chat.system.transform"]({ sessionID: "s2" }, { system: other });
	assert.equal(other.length, 1);
	assert.equal(callsWith(calls, "status").length, 2);
});

test("v1: a session with no namespace gets no pointer, and costs one read", async () => {
	const { exec, calls } = recordingExec(
		engine({ status: { stdout: JSON.stringify({ workspace: { namespace: null }, memory_count: 0 }) } }),
	);
	const hooks = await createPlugin({ exec, env: {} }).server({ directory: "/repo" });

	const system = [];
	await hooks["experimental.chat.system.transform"]({ sessionID: "s1" }, { system });
	await hooks["experimental.chat.system.transform"]({ sessionID: "s1" }, { system });

	assert.deepEqual(system, []);
	assert.equal(callsWith(calls, "status").length, 1);
});

test("v1: the pointer can be turned off", async () => {
	const { exec, calls } = recordingExec(engine());
	const hooks = await createPlugin({ exec, env: { [POINTER_ENV]: "0" } }).server({ directory: "/repo" });

	const system = [];
	await hooks["experimental.chat.system.transform"]({ sessionID: "s1" }, { system });

	assert.deepEqual(system, []);
	assert.deepEqual(calls, []);
});

test("v1: a compaction keeps the live state the transcript was carrying", async () => {
	const { exec, calls } = recordingExec(engine());
	const hooks = await createPlugin({ exec, env: {} }).server({ directory: "/repo" });

	const output = { context: [] };
	await hooks["experimental.session.compacting"]({ sessionID: "s1" }, output);

	assert.equal(
		argv(calls[0]),
		"mnexis list --type project_state,decision --full --limit 8 --json",
	);
	assert.equal(output.context.length, 1);
	assert.match(output.context[0], /The door answers previews/);
	assert.match(output.context[0], /8b4f9c2e-1111-4a1a-9f1f-1e2d3c4b5a69/);
});

test("v1: a refused capture never fails the turn and never swallows the refusal", async () => {
	const journal = [];
	const { exec } = recordingExec(engine({ prompt: { code: 4, stderr: "forbidden" } }));
	const hooks = await createPlugin({ exec, env: {}, logger: (record) => journal.push(record) }).server({
		directory: "/repo",
	});

	await hooks["chat.message"]({ sessionID: "s1" }, { message: { role: "user" }, parts: [{ type: "text", text: "hola" }] });

	assert.equal(journal.length, 1);
	assert.equal(journal[0].kind, "capture");
	assert.equal(journal[0].exit_code, 4);
	assert.equal(journal[0].bytes, 4);
	// The text never reaches the journal.
	assert.equal(JSON.stringify(journal[0]).includes("hola"), false);
});

// The unload path carries no reason on either model — V2's setup cleanup is
// `Cleanup = () => Promise<void> | void` and V1's `dispose()` takes no argument
// (`@opencode/plugin`'s `plugin.d.ts`; RESEARCH §1) — so a reload and a quit are
// the same event there. A V2 hot reload replaces the plugin, not the session,
// and the engine never reopens an ended session, so ending from the unload would
// kill a live one. The unload therefore kills what is in flight and stops; the
// close runs on the host's own end signal (`session.deleted`).
test("v1: unloading never ends a session: a reload replaces the plugin, not the session", async () => {
	const { exec, calls } = recordingExec(engine());
	const hooks = await createPlugin({ exec, env: {} }).server({ directory: "/repo" });

	await hooks.event({ event: { type: "session.created", properties: { info: { id: "s1" } } } });
	await hooks.event({ event: { type: "session.created", properties: { info: { id: "s2" } } } });
	const report = await hooks.dispose();

	assert.deepEqual(callsWith(calls, "end"), [], "no end: the host gave the unload no reason");
	assert.deepEqual(callsWith(calls, "save"), [], "no closing record either");
	assert.equal(report.action, "skipped");
	assert.match(report.reason, /reload/);
});

// --- the settle question (odd/tasks/opencode-parity.md, S3) ----------------
//
// The pi extension asks once, at the final boundary, when the session's own
// events show unsaved work. opencode promises no such boundary, so the ask
// rides the seat that is delivered per model request: the pointer's. The
// evidence is the outcome — a tool that failed changed no file and saved
// nothing — and the latch is one ask per session.

test("v1: a session that changed files and saved nothing is asked once, pointing at the taxonomy", async () => {
	const { exec } = recordingExec(engine());
	const hooks = await createPlugin({ exec, env: {} }).server({ directory: "/repo" });

	const system = [];
	await hooks["experimental.chat.system.transform"]({ sessionID: "s1" }, { system });
	assert.equal(system.length, 1, "nothing has changed yet: only the pointer is delivered");

	// A shell call that landed is the host's own observation that work happened.
	await hooks["tool.execute.after"]({ tool: "shell", sessionID: "s1" }, {});

	await hooks["experimental.chat.system.transform"]({ sessionID: "s1" }, { system });
	await hooks["experimental.chat.system.transform"]({ sessionID: "s1" }, { system });

	assert.equal(system.length, 2, "the pointer, then the question once");
	assert.equal(system[1], SETTLE_NUDGE);
	assert.match(system[1], /mnexis_memory_save/, "the ask names the tool that carries the taxonomy");
	assert.match(system[1], /memory_type/);
	assert.match(system[1], /knowledge/);
});

test("v1: a failed save is not a save, and a save that landed closes the question", async () => {
	const { exec } = recordingExec(engine());
	const hooks = await createPlugin({ exec, env: {} }).server({ directory: "/repo" });

	// s1: the save was attempted and failed, so the question stays open.
	await hooks["tool.execute.after"]({ tool: "write", sessionID: "s1" }, {});
	await hooks["tool.execute.after"]({ tool: "mnexis_memory_save", sessionID: "s1" }, { isError: true });
	const asked = [];
	await hooks["experimental.chat.system.transform"]({ sessionID: "s1" }, { system: asked });
	assert.ok(asked.includes(SETTLE_NUDGE));

	// s2: the save landed, so there is no question to ask.
	await hooks["tool.execute.after"]({ tool: "write", sessionID: "s2" }, {});
	await hooks["tool.execute.after"]({ tool: "mnexis_memory_save", sessionID: "s2" }, {});
	const quiet = [];
	await hooks["experimental.chat.system.transform"]({ sessionID: "s2" }, { system: quiet });
	assert.equal(quiet.includes(SETTLE_NUDGE), false);
});

test("v1: a tool that failed, or a tool that changed no file, earns no question", async () => {
	const { exec } = recordingExec(engine());
	const hooks = await createPlugin({ exec, env: {} }).server({ directory: "/repo" });

	await hooks["tool.execute.after"]({ tool: "edit", sessionID: "s1" }, { isError: true });
	await hooks["tool.execute.after"]({ tool: "read", sessionID: "s1" }, {});
	const system = [];
	await hooks["experimental.chat.system.transform"]({ sessionID: "s1" }, { system });

	assert.equal(system.includes(SETTLE_NUDGE), false);
});

// --- the session lifecycle (odd/tasks/opencode-parity.md, S3) -------------

test("v1: a session the engine already holds as ended is noted, not treated as an error", async () => {
	const journal = [];
	const { exec } = recordingExec(
		engine({ session: { stdout: JSON.stringify({ id: "s1", ended_at: "2026-10-01T00:00:00Z" }) } }),
	);
	const hooks = await createPlugin({ exec, env: {}, logger: (record) => journal.push(record) }).server({
		directory: "/repo",
	});

	await hooks.event({ event: { type: "session.created", properties: { info: { id: "s1" } } } });

	assert.deepEqual(
		journal.map((record) => [record.kind, record.already_ended, record.ended_at]),
		[["session-start", true, "2026-10-01T00:00:00Z"]],
		"a start never reopens a session: the engine's answer is reported, not hidden",
	);
});

test("v1: ending a session the engine never heard of is the engine's answer, not an exception", async () => {
	const journal = [];
	const { exec } = recordingExec(engine({ end: { code: 3, stderr: "not found" } }));
	const hooks = await createPlugin({ exec, env: {}, logger: (record) => journal.push(record) }).server({
		directory: "/repo",
	});

	await hooks.event({ event: { type: "session.created", properties: { info: { id: "s1" } } } });
	await hooks.event({ event: { type: "session.deleted", properties: { info: { id: "s1" } } } });

	const end = journal.find((record) => record.kind === "session-end");
	assert.equal(end.exit_code, 3);
	assert.equal(end.not_registered, true);
});

test("v1: a session the host deletes is ended once, and not again at unload", async () => {
	const { exec, calls } = recordingExec(engine());
	const hooks = await createPlugin({ exec, env: {} }).server({ directory: "/repo" });

	await hooks.event({ event: { type: "session.created", properties: { info: { id: "s1" } } } });
	await hooks.event({ event: { type: "session.deleted", properties: { info: { id: "s1" } } } });
	await hooks.dispose();

	assert.deepEqual(callsWith(calls, "end").map(argv), ["mnexis end --id s1 --json"]);
});

// --- the close: the state a session leaves behind (fix 2) -----------------
//
// pi writes one "estado al cierre" `project_state` at close (index.ts:2489-2520)
// and the engine's `end --json` answers the namespace it is written into
// (`src/cli/commands.rs:845-853`). The record goes through the same `run()`
// path every other write takes, so the write budget and the busy retry apply.

test("v1: the host's session.deleted writes the estado al cierre of that session", async () => {
	const { exec, calls } = recordingExec(engine());
	const hooks = await createPlugin({ exec, env: {} }).server({ directory: "/repo" });

	await hooks.event({ event: { type: "session.created", properties: { info: { id: "s1" } } } });
	const report = await hooks.event({
		event: { type: "session.deleted", properties: { info: { id: "s1" } } },
	});

	const save = callsWith(calls, "save")[0];
	assert.ok(save, "the closing state was written");
	assert.deepEqual(save.args.slice(0, 7), [
		"save",
		"--namespace",
		"acme",
		"--type",
		"project_state",
		"--title",
		"estado al cierre",
	]);
	const content = save.args[save.args.length - 2];
	assert.match(content, /Session s1 ended at 2026-11-03T10:30:00Z\./);
	assert.match(content, /It started at 2026-11-03T10:00:00Z\./);
	assert.match(content, /Directory: \/repo\./);
	// The end comes first: its answer is where the namespace comes from.
	assert.ok(
		calls.findIndex((call) => call.args[0] === "end") <
			calls.findIndex((call) => call.args[0] === "save"),
	);
	assert.equal(report.action, "closed");
	assert.equal(report.namespace, "acme");
});

// --- the shutdown budget (fix 3) -----------------------------------------
//
// The close is work a host hook awaits, so it gets one budget for all of its
// steps; a step that does not fit is abandoned, journaled, and the session stays
// tracked so the next close signal can retry it. The budget's own explanation is
// beside the v2 tests, where the abandonment is observed first.

test("v1: the abandoned close reaches the host's log and the close's own answer", async () => {
	const journal = [];
	const { exec } = recordingExec((args) => {
		if (args[0] === "end") return new Promise(() => {});
		return engine()(args);
	});
	const hooks = await createPlugin({
		exec,
		env: { [SHUTDOWN_BUDGET_ENV]: "60" },
		logger: (record) => journal.push(record),
	}).server({ directory: "/repo" });

	await hooks.event({ event: { type: "session.created", properties: { info: { id: "s1" } } } });
	const report = await hooks.event({
		event: { type: "session.deleted", properties: { info: { id: "s1" } } },
	});

	const close = journal.find((record) => record.kind === "session-end");
	assert.ok(close, "the abandonment is journaled, never swallowed");
	assert.equal(close.action, "abandoned");
	assert.match(close.reason, /60 ms shutdown budget/);
	assert.equal(report.action, "abandoned");
	assert.deepEqual(report.steps, [], "no step completed");
	assert.match(report.abandoned[0], /^end did not finish/);
});

test("an abandoned close is re-runnable: the next close signal tries again, under one title", async () => {
	let hang = true;
	const { exec, calls } = recordingExec((args) => {
		if (args[0] === "end" && hang) {
			hang = false;
			return new Promise(() => {});
		}
		return engine()(args);
	});
	const hooks = await createPlugin({ exec, env: { [SHUTDOWN_BUDGET_ENV]: "60" } }).server({
		directory: "/repo",
	});

	await hooks.event({ event: { type: "session.created", properties: { info: { id: "s1" } } } });
	const abandoned = await hooks.event({
		event: { type: "session.deleted", properties: { info: { id: "s1" } } },
	});
	assert.equal(abandoned.action, "abandoned");

	const retried = await hooks.event({
		event: { type: "session.deleted", properties: { info: { id: "s1" } } },
	});
	assert.equal(retried.action, "closed");
	// The record's title is the engine's identity: the retry revises the one
	// record instead of adding a second answer to "where did the session leave
	// the project".
	const titles = callsWith(calls, "save").map((call) => call.args[call.args.indexOf("--title") + 1]);
	assert.deepEqual(titles, ["estado al cierre"]);
});
