/**
 * The plugin, driven through both of opencode's plugin models.
 *
 * The host is faked — V1's `(input) => hooks` function and V2's `ctx` with its
 * domains — because the rules are the plugin's and the fakes make the exact CLI
 * argv visible. The real V2 loader is exercised separately, opt-in, in
 * `host.smoke.test.js`.
 */

import assert from "node:assert/strict";
import test from "node:test";

import plugin, {
	CURATED_DESCRIPTIONS,
	GRANT_REFUSAL,
	PLUGIN_ID,
	POINTER_ENV,
	PROFILE_REFUSAL,
	SETTLE_NUDGE,
	TYPE_GLOSSES,
	createPlugin,
} from "../index.js";

const STATUS_JSON = JSON.stringify({
	workspace: { key: "git:example.com-acme", namespace: "acme", candidate_namespace: null },
	memory_count: 42,
});

const PREVIOUS_JSON = JSON.stringify({
	session: { id: "sess-previous", started_at: "2026-10-30T22:04:11Z", summary: "Wired the budget." },
});

const STATE_JSON = JSON.stringify({
	count: 1,
	items: [
		{
			uuid: "8b4f9c2e-1111-4a1a-9f1f-1e2d3c4b5a69",
			title: "The door answers previews",
			memory_type: "decision",
			revision: 3,
			content: "memory_context answers a preview per item.",
		},
	],
});

// --- helpers ---------------------------------------------------------------

/** An exec that records every call and answers from `responder`. */
function recordingExec(responder = () => undefined) {
	const calls = [];
	const exec = async (command, args, options) => {
		calls.push({ command, args, cwd: options?.cwd });
		const answer = responder(args, options) ?? {};
		return { code: answer.code ?? 0, stdout: answer.stdout ?? "", stderr: answer.stderr ?? "" };
	};
	return { exec, calls };
}

/** The CLI calls that carry every `needle`. */
function callsWith(calls, ...needles) {
	return calls.filter((call) => needles.every((needle) => call.args.includes(needle)));
}

function argv(call) {
	return [call.command, ...call.args].join(" ");
}

/** Wait until `predicate` holds; the event stream is asynchronous by contract. */
async function waitFor(predicate, { timeout = 1000 } = {}) {
	const started = Date.now();
	while (Date.now() - started < timeout) {
		if (predicate()) return true;
		await new Promise((resolve) => setTimeout(resolve, 5));
	}
	return predicate();
}

/** The responder every test starts from: the engine answers, the writes succeed. */
function engine(overrides = {}) {
	return (args) => {
		const key = args[0];
		if (key === "status") return { stdout: STATUS_JSON, ...overrides.status };
		if (key === "recall") return { stdout: PREVIOUS_JSON, ...overrides.recall };
		if (key === "list") return { stdout: STATE_JSON, ...overrides.list };
		if (key === "prompt") return { stdout: "{}", ...overrides.prompt };
		if (key === "session") return { stdout: "{}", ...overrides.session };
		if (key === "end") return { stdout: "{}", ...overrides.end };
		return undefined;
	};
}

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

test("v1: unloading ends every session the plugin opened, once", async () => {
	const { exec, calls } = recordingExec(engine());
	const hooks = await createPlugin({ exec, env: {} }).server({ directory: "/repo" });

	await hooks.event({ event: { type: "session.created", properties: { info: { id: "s1" } } } });
	await hooks.event({ event: { type: "session.created", properties: { info: { id: "s2" } } } });
	await hooks.event({ event: { type: "session.created", properties: { info: { id: "s2" } } } });
	await hooks.dispose();

	const ends = callsWith(calls, "end").map(argv);
	assert.deepEqual(ends, ["mnexis end --id s1 --json", "mnexis end --id s2 --json"]);
});

// --- opencode 2.x ---------------------------------------------------------

/**
 * The host's ToolEditor, as V2 exposes it (`@opencode/plugin`'s ToolEditor):
 * `update` mutates the listed tool, and an id the catalog does not carry is
 * ignored — never an exception.
 */
function fakeEditor(tools) {
	const catalog = new Map();
	for (const tool of tools) {
		catalog.set(tool.id, { description: "", ...tool });
	}
	return {
		list: () => [...catalog.values()],
		get: (id) => catalog.get(id),
		update(id, update) {
			const tool = catalog.get(id);
			if (!tool) return;
			update(tool);
		},
	};
}

/**
 * A V2 host: the domains record the hooks the plugin registers, the catalog
 * transforms it registers, and the event stream is a queue the test can push
 * into.
 */
function fakeHost({ directory = "/repo" } = {}) {
	const hooks = new Map();
	const transforms = [];
	const queue = [];
	let wake = null;
	let closed = false;

	const ctx = {
		location: { directory },
		session: {
			hook: async (name, callback) => {
				hooks.set(`session:${name}`, callback);
				const registration = { dispose: async () => hooks.delete(`session:${name}`) };
				return registration;
			},
		},
		tool: {
			hook: async (name, callback) => {
				hooks.set(`tool:${name}`, callback);
				return { dispose: async () => hooks.delete(`tool:${name}`) };
			},
			transform: async (callback) => {
				transforms.push(callback);
				return { dispose: async () => transforms.splice(transforms.indexOf(callback), 1) };
			},
		},
		event: {
			subscribe: ({ signal } = {}) =>
				(async function* stream() {
					for (;;) {
						if (queue.length > 0) {
							yield queue.shift();
							continue;
						}
						if (closed || signal?.aborted) {
							return;
						}
						await new Promise((resolve) => {
							wake = resolve;
						});
					}
				})(),
		},
	};

	return {
		ctx,
		hooks,
		transforms,
		/** Run every registered transform over a fresh catalog of `tools`. */
		transform(tools) {
			const editor = fakeEditor(tools);
			for (const callback of transforms) {
				callback(editor);
			}
			return editor;
		},
		/** Run every registered transform over an editor the test supplies. */
		runTransform(editor) {
			for (const callback of transforms) {
				callback(editor);
			}
			return editor;
		},
		push(event) {
			queue.push(event);
			if (wake) {
				wake();
				wake = null;
			}
		},
		close() {
			closed = true;
			if (wake) {
				wake();
				wake = null;
			}
		},
	};
}

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

test("v2: the cleanup kills the calls still in flight before it ends the sessions", async () => {
	const events = [];
	const registry = { killAll: () => events.push("kill") };
	const { exec } = recordingExec((args) => {
		events.push(args[0]);
		return {};
	});
	const host = fakeHost();
	const cleanup = await createPlugin({ exec, env: {}, registry }).setup(host.ctx);

	await host.hooks.get("session:prompt")({ sessionID: "s1", prompt: { text: "hola" } });
	host.close();
	await cleanup();

	assert.deepEqual(events, ["session", "prompt", "kill", "end"]);
});

test("v2: the cleanup ends every session the plugin opened", async () => {
	const { exec, calls } = recordingExec(engine());
	const host = fakeHost();
	const cleanup = await createPlugin({ exec, env: {} }).setup(host.ctx);

	await host.hooks.get("session:prompt")({ sessionID: "s1", prompt: { text: "hola" } });
	host.close();
	await cleanup();

	assert.deepEqual(callsWith(calls, "end").map(argv), ["mnexis end --id s1 --json"]);
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

// --- the model-facing catalog (V2 only) -----------------------------------

test("v2: the catalog seat sits beside the tool gate", async () => {
	const { exec } = recordingExec(engine());
	const host = fakeHost();
	const cleanup = await createPlugin({ exec, env: {} }).setup(host.ctx);

	assert.equal(host.transforms.length, 1);
	assert.ok(host.hooks.has("tool:execute.before"));

	host.close();
	await cleanup();
});

test("v2: a catalog the transform cannot read never breaks the turn", async () => {
	const { exec } = recordingExec(engine());
	const host = fakeHost();
	const cleanup = await createPlugin({ exec, env: {} }).setup(host.ctx);

	// A measured edge: an uncaught throw inside this callback corrupts the
	// turn, so every shape of a catalog this plugin cannot read is swallowed
	// and the host keeps the definitions it built.
	for (const editor of [
		{ list() { throw new Error("no catalog"); } },
		{ list: () => null },
		{ list: () => [{ id: "mnexis_memory_save", description: "", options: {} }] },
		null,
	]) {
		assert.doesNotThrow(() => host.runTransform(editor));
	}

	host.close();
	await cleanup();
});

test("v2: the transform rewrites the description of every mnexis tool the map knows", async () => {
	const { exec } = recordingExec(engine());
	const host = fakeHost();
	const cleanup = await createPlugin({ exec, env: {} }).setup(host.ctx);

	const editor = host.transform([
		{
			id: "mnexis_memory_save",
			description: "Save a memory that must survive the session.",
			options: { namespace: "mnexis", codemode: true },
		},
		{
			id: "mnexis_memory_context",
			description: "Call this FIRST when you are about to work.",
			options: { namespace: "mnexis", codemode: true },
		},
		{ id: "mnexis_retired_tool", description: "The engine renamed this one.", options: {} },
		{ id: "bash", description: "Run a shell command." },
	]);

	assert.equal(
		editor.get("mnexis_memory_save").description,
		CURATED_DESCRIPTIONS.mnexis_memory_save,
	);
	assert.equal(
		editor.get("mnexis_memory_context").description,
		CURATED_DESCRIPTIONS.mnexis_memory_context,
	);

	// The seven kinds ride the save, one per line, exactly as pi states them.
	const save = editor.get("mnexis_memory_save").description.split("\n");
	assert.equal(TYPE_GLOSSES.length, 7);
	for (const gloss of TYPE_GLOSSES) {
		assert.ok(save.includes(gloss), `the save carries the gloss: ${gloss}`);
	}

	// An id the map does not know is skipped; a tool that is not mnexis's is
	// never touched.
	assert.equal(editor.get("mnexis_retired_tool").description, "The engine renamed this one.");
	assert.equal(editor.get("bash").description, "Run a shell command.");

	host.close();
	await cleanup();
});

test("v2: the transform turns Code Mode off for every mnexis tool, and nothing else", async () => {
	const { exec } = recordingExec(engine());
	const host = fakeHost();
	const cleanup = await createPlugin({ exec, env: {} }).setup(host.ctx);

	const editor = host.transform([
		{
			id: "mnexis_memory_save",
			description: "",
			options: { namespace: "mnexis", codemode: true },
		},
		{ id: "mnexis_memory_workspace", description: "" },
		{ id: "mnexis_retired_tool", description: "", options: { namespace: "mnexis" } },
		{ id: "bash", description: "Run a shell command.", options: { codemode: true } },
	]);

	// The host's own keys survive; only codemode moves.
	assert.deepEqual(editor.get("mnexis_memory_save").options, {
		namespace: "mnexis",
		codemode: false,
	});
	// A tool the host gave no options keeps working, with the one field.
	assert.deepEqual(editor.get("mnexis_memory_workspace").options, { codemode: false });
	// An id the map does not know is still mnexis's: it stays reachable.
	assert.deepEqual(editor.get("mnexis_retired_tool").options, {
		namespace: "mnexis",
		codemode: false,
	});
	// A tool that is not mnexis's gets no options invented for it.
	assert.deepEqual(editor.get("bash").options, { codemode: true });

	host.close();
	await cleanup();
});

test("v2: a catalog with no mnexis tools is left exactly as it is", async () => {
	const { exec } = recordingExec(engine());
	const host = fakeHost();
	const cleanup = await createPlugin({ exec, env: {} }).setup(host.ctx);

	const tools = [
		{ id: "bash", description: "Run a shell command.", options: { codemode: true } },
		{ id: "read", description: "Read a file." },
	];
	const editor = host.transform(tools);

	assert.deepEqual(editor.list(), tools);

	host.close();
	await cleanup();
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
	await hooks.dispose();

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
