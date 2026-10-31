/**
 * Thin, injectable wrapper around the mnexis CLI.
 *
 * The exec function is injected so unit tests never need the binary and
 * never touch the user's profile.
 */

import { spawn } from "node:child_process";

/**
 * How long one engine call may take before the extension stops it (S2, audit
 * A9): a read is one indexed query against a local store, so ten seconds is
 * already pathological — and the price of waiting is a turn, a save or a
 * session hook the human is blocked on.
 */
export const READ_TIMEOUT_MS = 10_000;

/**
 * A write may wait on the store's lock and pay for its fsync, and a save the
 * engine is still committing is worth patience — thirty seconds of it, never
 * forever.
 */
export const WRITE_TIMEOUT_MS = 30_000;

/** How long a killed child has to honour SIGTERM before it gets SIGKILL. */
const KILL_GRACE_MS = 200;

/** The most output one call may accumulate, as the execFile cap it replaced. */
const MAX_OUTPUT_BYTES = 8 * 1024 * 1024;

/**
 * The two-word commands this client talks to. An answer names the call it gave
 * up on, and `project` alone would name the group instead of the command.
 */
const SUBCOMMANDS = new Set([
	"project list",
	"project bind",
	"config show",
	"config allow-namespace",
]);

/** The commands that change the store or the profile: the longer budget. */
const WRITE_COMMANDS = new Set([
	"init",
	"save",
	"supersede",
	"prompt",
	"session",
	"end",
	"project bind",
	"config allow-namespace",
]);

/** The name of a call, the way an answer says it: `search`, `project list`. */
function commandName(args) {
	const first = String(args[0] ?? "");
	const joined = args.length > 1 ? `${first} ${args[1]}` : first;
	return SUBCOMMANDS.has(joined) ? joined : first;
}

/** One call's budget: the write one for a write, the read one otherwise. */
function timeoutFor(args, override) {
	if (override !== null) {
		return override;
	}
	return WRITE_COMMANDS.has(commandName(args)) ? WRITE_TIMEOUT_MS : READ_TIMEOUT_MS;
}

/** The budget `MNEXIS_TIMEOUT_MS` overrides both defaults with, or null. */
function overrideOf(env) {
	const value = Number.parseInt(String(env?.MNEXIS_TIMEOUT_MS ?? ""), 10);
	return Number.isFinite(value) && value > 0 ? value : null;
}

/**
 * Signal everything the child started, not only the child.
 *
 * The engine runs in a process group of its own (`detached` below), so `-pid`
 * is this one call and every process it spawned: pi's abort has to stop the
 * work, not leave it running behind a dead parent (S2).
 */
function killGroup(child, signal) {
	try {
		if (process.platform === "win32") {
			child.kill(signal);
		} else {
			process.kill(-child.pid, signal);
		}
	} catch {
		try {
			child.kill(signal);
		} catch {
			// Already gone: a kill that finds nothing has nothing to do.
		}
	}
}

/**
 * Default exec: resolves with { code, stdout, stderr } and never rejects.
 *
 * Two things end a call the engine does not answer: the `timeout` this client
 * passes, and the caller's `signal`. Both kill the child's process group,
 * because a hung `mnexis` used to block `session_start`, every `input` hook and
 * every save, and pi's abort could not reach the child (S2, audit A9).
 */
export function defaultExec(command, args, options = {}) {
	const { cwd, env, signal, timeout } = options;
	const timeoutMs = Number.isFinite(timeout) && timeout > 0 ? timeout : 0;
	return new Promise((resolve) => {
		if (signal && signal.aborted) {
			resolve({ code: 1, stdout: "", stderr: "", cancelled: true });
			return;
		}

		let stdout = "";
		let stderr = "";
		let bytes = 0;
		let settled = false;
		let killedFor = null;
		let timer = null;
		let grace = null;

		const child = spawn(command, args, {
			cwd,
			env,
			// A group of its own: `-pid` is then this call and everything it
			// spawned, never the extension's own group (POSIX only).
			detached: process.platform !== "win32",
			stdio: ["ignore", "pipe", "pipe"],
		});

		const finish = (code) => {
			if (settled) {
				return;
			}
			settled = true;
			if (timer) {
				clearTimeout(timer);
			}
			if (grace) {
				clearTimeout(grace);
			}
			if (signal && typeof signal.removeEventListener === "function") {
				signal.removeEventListener("abort", onAbort);
			}
			resolve({
				code: killedFor === null ? code : 1,
				stdout,
				stderr,
				...(killedFor === null
					? {}
					: { timed_out: killedFor === "timeout", cancelled: killedFor === "cancelled" }),
			});
		};

		const stop = (why) => {
			if (settled || killedFor !== null) {
				return;
			}
			killedFor = why;
			killGroup(child, "SIGTERM");
			// A process that ignores SIGTERM is still a hung engine.
			grace = setTimeout(() => killGroup(child, "SIGKILL"), KILL_GRACE_MS);
			if (typeof grace.unref === "function") {
				grace.unref();
			}
		};

		const onAbort = () => stop("cancelled");

		if (timeoutMs > 0) {
			timer = setTimeout(() => stop("timeout"), timeoutMs);
		}
		if (signal && typeof signal.addEventListener === "function") {
			signal.addEventListener("abort", onAbort);
		}

		if (child.stdout) {
			child.stdout.setEncoding("utf8");
			child.stdout.on("data", (chunk) => {
				stdout += chunk;
				bytes += Buffer.byteLength(chunk, "utf8");
				if (bytes > MAX_OUTPUT_BYTES) {
					stderr += `the engine answered more than ${MAX_OUTPUT_BYTES} bytes\n`;
					killGroup(child, "SIGKILL");
				}
			});
		}
		if (child.stderr) {
			child.stderr.setEncoding("utf8");
			child.stderr.on("data", (chunk) => {
				stderr += chunk;
			});
		}
		child.on("error", (error) => {
			const missing = Boolean(error) && error.code === "ENOENT";
			stderr +=
				(missing ? `${command}: command not found (ENOENT)\n` : "") +
				(error instanceof Error ? error.message : String(error));
			finish(missing ? 127 : 1);
		});
		child.on("close", (code) => finish(typeof code === "number" ? code : 1));
	});
}

/** Parse CLI JSON output, tolerating empty or noisy output. */
export function parseJson(stdout) {
	const text = String(stdout ?? "").trim();
	if (text === "") {
		return null;
	}
	try {
		return JSON.parse(text);
	} catch {
		const start = text.indexOf("{");
		const end = text.lastIndexOf("}");
		if (start >= 0 && end > start) {
			try {
				return JSON.parse(text.slice(start, end + 1));
			} catch {
				return null;
			}
		}
		return null;
	}
}

/**
 * What a JSON call did not get, or null when it got an answer.
 *
 * An engine that exits 0 having printed nothing and an engine that exits 0
 * having printed something this extension cannot read are two different
 * failures, and neither of them is data (S3, audit A8).
 */
function noAnswerOf(json, stdout, data) {
	if (!json || data !== null) {
		return null;
	}
	return String(stdout ?? "").trim() === "" ? "empty" : "unreadable";
}

/**
 * Create a CLI client.
 *
 * The client takes no signal: `client.withSignal(signal)` answers a client
 * bound to pi's abort signal for one tool call, so a cancelled tool kills the
 * engine it started while the rest of the session keeps its own calls.
 *
 * @param {{ exec?: Function, binary?: string, env?: Record<string, string|undefined> }} options
 */
export function createClient({ exec = defaultExec, binary, env = process.env } = {}) {
	const command = binary || env.MNEXIS_BIN || "mnexis";
	const budget = overrideOf(env);

	async function run(args, { json = false, cwd, signal = null } = {}) {
		const fullArgs = json ? [...args, "--json"] : args;
		const timeout = timeoutFor(args, budget);
		const result = await exec(command, fullArgs, {
			cwd,
			env: {
				...env,
				...(env.MNEXIS_DATA_DIR ? { MNEXIS_DATA_DIR: env.MNEXIS_DATA_DIR } : {}),
			},
			signal,
			timeout,
		});
		const data = json ? parseJson(result.stdout) : null;
		return {
			...result,
			// The call this answer belongs to, the budget it had, and what it
			// did not say: an answer that carries none of them is a call the
			// tool cannot report honestly (S2, S3).
			command: commandName(args),
			timeout_ms: timeout,
			data,
			no_answer: noAnswerOf(json, result.stdout, data),
		};
	}

	/** A git command inside the workspace: a bounded read like any other. */
	async function runGit(args, cwd, signal) {
		const result = await exec("git", args, {
			cwd,
			signal,
			timeout: budget === null ? READ_TIMEOUT_MS : budget,
		});
		return {
			...result,
			value: result.code === 0 ? result.stdout.trim() : null,
		};
	}

	const client = {
		command,
		run,

		/**
		 * The same client, with pi's abort signal bound to every call it makes.
		 *
		 * Every method below asks `this.run`, so this object is the whole
		 * difference: a cancelled tool kills the engine it started, and a tool
		 * with no signal is the client it always was (S2).
		 */
		withSignal(signal) {
			if (!signal) {
				return client;
			}
			const bound = Object.create(client);
			bound.run = (args, options = {}) => run(args, { ...options, signal });
			bound.git = (args, cwd) => runGit(args, cwd, signal);
			return bound;
		},

		async init(cwd) {
			return this.run(["init"], { cwd });
		},

		async projectList(cwd) {
			return this.run(["project", "list"], { json: true, cwd });
		},

		async bind(namespace, key, path, cwd) {
			return this.run(["project", "bind", namespace, "--key", key, "--path", path], { cwd });
		},

		async allowNamespace(namespace, cwd) {
			return this.run(["config", "allow-namespace", namespace], { cwd });
		},

		async configShow(cwd) {
			return this.run(["config", "show"], { json: true, cwd });
		},

		async save({ namespace, content, title, tags, keywords, topicKey, idempotencyKey, type }, cwd) {
			const args = ["save", "--namespace", namespace];
			if (typeof type === "string" && type !== "") {
				args.push("--type", type);
			}
			if (title) {
				args.push("--title", title);
			}
			if (tags) {
				args.push("--tags", tags);
			}
			if (keywords) {
				args.push("--keywords", keywords);
			}
			if (topicKey) {
				args.push("--topic-key", topicKey);
			}
			if (idempotencyKey) {
				args.push("--idempotency-key", idempotencyKey);
			}
			args.push(content);
			return this.run(args, { json: true, cwd });
		},

		async search({ query, namespace, limit, match, type }, cwd) {
			const args = ["search", query];
			if (namespace) {
				args.push("--namespace", namespace);
			}
			if (typeof limit === "number") {
				args.push("--limit", String(limit));
			}
			if (typeof match === "string" && match !== "") {
				args.push("--match", match);
			}
			if (typeof type === "string" && type !== "") {
				args.push("--type", type);
			}
			return this.run(args, { json: true, cwd });
		},

		/**
		 * The budgeted one-call door: the task-shaped question, answered
		 * inside a token budget.
		 */
		async context({ task, namespace, tokenBudget, maxItems, match }, cwd) {
			const args = ["context"];
			if (namespace) {
				args.push("--namespace", namespace);
			}
			if (typeof tokenBudget === "number") {
				args.push("--token-budget", String(tokenBudget));
			}
			if (typeof maxItems === "number") {
				args.push("--max-items", String(maxItems));
			}
			if (typeof match === "string" && match !== "") {
				args.push("--match", match);
			}
			args.push(task);
			return this.run(args, { json: true, cwd });
		},

		async list({ namespace, limit, full, type, value, offset }, cwd) {
			const args = ["list"];
			if (namespace) {
				args.push("--namespace", namespace);
			}
			if (typeof limit === "number") {
				args.push("--limit", String(limit));
			}
			if (typeof offset === "number") {
				args.push("--offset", String(offset));
			}
			if (full === true) {
				args.push("--full");
			}
			if (typeof type === "string" && type !== "") {
				args.push("--type", type);
			}
			if (typeof value === "string" && value !== "") {
				args.push("--value", value);
			}
			return this.run(args, { json: true, cwd });
		},

		async sessionStart({ sessionId, namespace, directory }, cwd) {
			const args = ["session", "--id", sessionId];
			if (namespace) {
				args.push("--namespace", namespace);
			}
			if (directory) {
				args.push("--directory", directory);
			}
			return this.run(args, { json: true, cwd });
		},

		async sessionEnd({ sessionId, summary }, cwd) {
			const args = ["end", "--id", sessionId];
			if (summary) {
				args.push("--summary", summary);
			}
			return this.run(args, { json: true, cwd });
		},

		async savePrompt({ content, sessionId, namespace }, cwd) {
			const args = ["prompt", content];
			if (sessionId) {
				args.push("--session", sessionId);
			}
			if (namespace) {
				args.push("--namespace", namespace);
			}
			return this.run(args, { json: true, cwd });
		},

		async searchPrompts({ query, matchMode, limit }, cwd) {
			const args = ["prompts", "--search", query];
			if (matchMode) {
				args.push("--match", matchMode);
			}
			if (typeof limit === "number") {
				args.push("--limit", String(limit));
			}
			return this.run(args, { json: true, cwd });
		},

		/** The recall of a session: what it was, in one answer. */
		async recall({ session, previous, full }, cwd) {
			const args = ["recall"];
			if (session) {
				args.push("--session", session);
			}
			if (previous) {
				args.push("--previous");
			}
			if (full === true) {
				args.push("--full");
			}
			return this.run(args, { json: true, cwd });
		},

		/** Where this workspace may write, and what is already there. */
		async status(cwd) {
			return this.run(["status"], { json: true, cwd });
		},

		/** The revision history of one memory. */
		async history({ uuid, limit }, cwd) {
			const args = ["history", uuid];
			if (typeof limit === "number") {
				args.push("--limit", String(limit));
			}
			return this.run(args, { json: true, cwd });
		},

		async suggestTopicKey({ title, namespace }, cwd) {
			const args = ["suggest-topic-key", title];
			if (namespace) {
				args.push("--namespace", namespace);
			}
			return this.run(args, { json: true, cwd });
		},

		async show(uuid, cwd) {
			return this.run(["show", uuid], { json: true, cwd });
		},

		async supersede({ older, newer, reason }, cwd) {
			return this.run(["supersede", "--older", older, "--newer", newer, "--reason", reason], { cwd });
		},

		/** Git helpers: run a git command inside the workspace. */
		async git(args, cwd) {
			return runGit(args, cwd, null);
		},
	};

	return client;
}
