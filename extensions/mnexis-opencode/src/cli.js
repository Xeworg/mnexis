// --- the CLI ---------------------------------------------------------------

/** Default exec: resolves with `{ code, stdout, stderr }` and never rejects. */
export function defaultExec(command, args, options = {}) {
	const { registry, ...spawnOptions } = options;
	return new Promise((resolve) => {
		const child = nodeExecFile(
			command,
			args,
			{ encoding: "utf8", maxBuffer: 8 * 1024 * 1024, ...spawnOptions },
			(error, stdout, stderr) => {
				let code = 0;
				if (error) {
					code = typeof error.code === "number" ? error.code : 1;
					if (error.code === "ENOENT") {
						code = 127;
					}
				}
				resolve({ code, stdout: stdout ?? "", stderr: stderr ?? "" });
			},
		);
		if (registry && typeof registry.track === "function") {
			registry.track(child);
		}
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

/** The budget one call gets: the write one for a write, the read one otherwise. */
function timeoutFor(name) {
	return WRITE_COMMANDS.has(name) ? WRITE_TIMEOUT_MS : READ_TIMEOUT_MS;
}

/**
 * Create the CLI client.
 *
 * `exec` is injected by the tests, so neither the binary nor a profile is
 * needed to exercise it. Every call carries its own budget, and the commands
 * whose identity the engine owns retry a locked store; `withBudget(ms)` answers
 * a client whose calls are bounded by a smaller budget (the work a host hook
 * awaits without a timeout of its own).
 */
export function createClient({ exec = defaultExec, binary, env = process.env, registry = null } = {}) {
	const command = binary || env.MNEXIS_BIN || "mnexis";

	async function run(args, { json = false, cwd, timeout: override = null } = {}) {
		const fullArgs = json ? [...args, "--json"] : args;
		const name = String(args[0] ?? "");
		const timeout = Number.isFinite(override) && override > 0 ? override : timeoutFor(name);
		// The retry belongs to the command, not to the caller: a command that is
		// idempotent by construction always retries, a read pays one call.
		const retry = RETRY_COMMANDS.has(name);
		const deadline = Date.now() + timeout;
		const call = (budgetMs) => exec(command, fullArgs, { cwd, env, registry, timeout: budgetMs });

		let result = await call(timeout);
		let busyRetries = 0;
		while (
			retry &&
			result.code === BUSY_EXIT_CODE &&
			busyRetries < BUSY_RETRY_DELAYS_MS.length
		) {
			const delay = BUSY_RETRY_DELAYS_MS[busyRetries];
			if (deadline - Date.now() <= delay) {
				break;
			}
			await sleep(delay);
			const left = deadline - Date.now();
			if (left <= 0) {
				break;
			}
			result = await call(left);
			busyRetries += 1;
		}

		const data = json ? parseJson(result.stdout) : null;
		return {
			...result,
			// The call this answer belongs to, the budget it had, and how many
			// times a locked store was tried again: a journal reads these to tell
			// a recovered call from a lost one.
			command: name,
			timeout_ms: timeout,
			busy_retries: busyRetries,
			data,
		};
	}

	function make(binding = {}) {
		const call = (args, options = {}) => run(args, { ...binding, ...options });

		return {
			command,
			run: call,

			/** The same client, with every call bounded by `ms`. */
			withBudget(ms) {
				return make({ ...binding, timeout: ms });
			},

			/** Where this workspace may write, and what is already there. */
			async status(cwd) {
				return call(["status"], { json: true, cwd });
			},

			/** The recall of the session before this one. */
			async previousSession(cwd) {
				return call(["recall", "--previous"], { json: true, cwd });
			},

			/** The live records that must survive a compaction, with their content. */
			async stateRecords(cwd, limit = STATE_LIMIT) {
				return call(
					["list", "--type", "project_state,decision", "--full", "--limit", String(limit)],
					{ json: true, cwd },
				);
			},

			async sessionStart({ sessionId, directory }, cwd) {
				const args = ["session", "--id", sessionId];
				if (directory) {
					args.push("--directory", directory);
				}
				return call(args, { json: true, cwd });
			},

			async sessionEnd({ sessionId }, cwd) {
				return call(["end", "--id", sessionId], { json: true, cwd });
			},

			async savePrompt({ content, sessionId }, cwd) {
				const args = ["prompt", content];
				if (sessionId) {
					args.push("--session", sessionId);
				}
				return call(args, { json: true, cwd });
			},

			/**
			 * One typed record, in the namespace the engine named.
			 *
			 * The title is the record's identity, so the second try of a locked
			 * store is the same record and never a second one — the reason `save`
			 * carries the busy retry and the write budget.
			 */
			async saveMemory({ namespace, title, type, content }, cwd) {
				const args = ["save", "--namespace", namespace];
				if (typeof type === "string" && type !== "") {
					args.push("--type", type);
				}
				if (typeof title === "string" && title !== "") {
					args.push("--title", title);
				}
				args.push(content);
				return call(args, { json: true, cwd });
			},
		};
	}

	return make();
}
