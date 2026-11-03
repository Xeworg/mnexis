/**
 * The host harness both plugin models' tests share.
 *
 * The host is faked — V1's `(input) => hooks` function and V2's `ctx` with its
 * domains — because the rules are the plugin's and the fakes make the exact CLI
 * argv visible. The real V2 loader is exercised separately, opt-in, in
 * `host.smoke.test.js`. This file is not a test suite: `test/*.test.js` is what
 * the runner picks up.
 */

export const STATUS_JSON = JSON.stringify({
	workspace: { key: "git:example.com-acme", namespace: "acme", candidate_namespace: null },
	memory_count: 42,
});

export const PREVIOUS_JSON = JSON.stringify({
	session: { id: "sess-previous", started_at: "2026-10-30T22:04:11Z", summary: "Wired the budget." },
});

export const ENDED_JSON = (id) =>
	JSON.stringify({
		id,
		namespace: "acme",
		directory: "/repo",
		started_at: "2026-11-03T10:00:00Z",
		ended_at: "2026-11-03T10:30:00Z",
	});

export const STATE_JSON = JSON.stringify({
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

/**
 * An exec that records every call and answers from `responder`.
 *
 * The answer is awaited, so a responder may hold a call open — the one shape a
 * budget is tested with.
 */
export function recordingExec(responder = () => undefined) {
	const calls = [];
	const exec = async (command, args, options) => {
		calls.push({ command, args, cwd: options?.cwd });
		const answer = (await responder(args, options)) ?? {};
		return { code: answer.code ?? 0, stdout: answer.stdout ?? "", stderr: answer.stderr ?? "" };
	};
	return { exec, calls };
}

/** The CLI calls that carry every `needle`. */
export function callsWith(calls, ...needles) {
	return calls.filter((call) => needles.every((needle) => call.args.includes(needle)));
}

export function argv(call) {
	return [call.command, ...call.args].join(" ");
}

/** Wait until `predicate` holds; the event stream is asynchronous by contract. */
export async function waitFor(predicate, { timeout = 1000 } = {}) {
	const started = Date.now();
	while (Date.now() - started < timeout) {
		if (predicate()) return true;
		await new Promise((resolve) => setTimeout(resolve, 5));
	}
	return predicate();
}

/** The responder every test starts from: the engine answers, the writes succeed. */
export function engine(overrides = {}) {
	return (args) => {
		const key = args[0];
		if (key === "status") return { stdout: STATUS_JSON, ...overrides.status };
		if (key === "recall") return { stdout: PREVIOUS_JSON, ...overrides.recall };
		if (key === "list") return { stdout: STATE_JSON, ...overrides.list };
		if (key === "prompt") return { stdout: "{}", ...overrides.prompt };
		if (key === "session") return { stdout: "{}", ...overrides.session };
		if (key === "end") {
			// The engine answers the session it ended: `end --json` carries the
			// namespace the closing record is written into (`src/cli/commands.rs`).
			const at = args.indexOf("--id");
			const id = at >= 0 ? args[at + 1] : "s1";
			return { stdout: ENDED_JSON(id), ...overrides.end };
		}
		if (key === "save") return { stdout: "{}", ...overrides.save };
		return undefined;
	};
}

/**
 * The host's ToolEditor, as V2 exposes it (`@opencode/plugin`'s ToolEditor):
 * `update` mutates the listed tool, and an id the catalog does not carry is
 * ignored — never an exception.
 */
export function fakeEditor(tools) {
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
 * into. `storage` is the journal this host has instead of a logger
 * (`StorageDomain`: `get`/`set`/`remove`/`scan`).
 */
export function fakeHost({ directory = "/repo" } = {}) {
	const hooks = new Map();
	const transforms = [];
	const queue = [];
	const journal = new Map();
	let wake = null;
	let closed = false;

	const ctx = {
		location: { directory },
		storage: {
			get: async (key) => journal.get(key),
			set: async (key, value) => {
				journal.set(key, value);
			},
			remove: async (key) => {
				journal.delete(key);
			},
			scan: async () => ({ entries: [...journal.keys()] }),
		},
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
		journal,
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
