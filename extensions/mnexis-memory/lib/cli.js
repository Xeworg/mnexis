/**
 * Thin, injectable wrapper around the mnexis CLI.
 *
 * The exec function is injected so unit tests never need the binary and
 * never touch the user's profile.
 */

import { execFile as nodeExecFile } from "node:child_process";

/** Default exec: resolves with { code, stdout, stderr } and never rejects. */
export function defaultExec(command, args, options = {}) {
	return new Promise((resolve) => {
		nodeExecFile(
			command,
			args,
			{ encoding: "utf8", maxBuffer: 8 * 1024 * 1024, ...options },
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
 * Create a CLI client.
 *
 * @param {{ exec?: Function, binary?: string, env?: Record<string, string|undefined> }} options
 */
export function createClient({ exec = defaultExec, binary, env = process.env } = {}) {
	const command = binary || env.MNEXIS_BIN || "mnexis";

	async function run(args, { json = false, cwd } = {}) {
		const fullArgs = json ? [...args, "--json"] : args;
		const result = await exec(command, fullArgs, {
			cwd,
			env: {
				...env,
				...(env.MNEXIS_DATA_DIR ? { MNEXIS_DATA_DIR: env.MNEXIS_DATA_DIR } : {}),
			},
		});
		return {
			...result,
			data: json ? parseJson(result.stdout) : null,
		};
	}

	return {
		command,
		run,

		async init(cwd) {
			return run(["init"], { cwd });
		},

		async projectList(cwd) {
			return run(["project", "list"], { json: true, cwd });
		},

		async bind(namespace, key, path, cwd) {
			return run(["project", "bind", namespace, "--key", key, "--path", path], { cwd });
		},

		async allowNamespace(namespace, cwd) {
			return run(["config", "allow-namespace", namespace], { cwd });
		},

		async configShow(cwd) {
			return run(["config", "show"], { json: true, cwd });
		},

		async save({ namespace, content, tags, topicKey, idempotencyKey }, cwd) {
			const args = ["save", "--namespace", namespace];
			if (tags) {
				args.push("--tags", tags);
			}
			if (topicKey) {
				args.push("--topic-key", topicKey);
			}
			if (idempotencyKey) {
				args.push("--idempotency-key", idempotencyKey);
			}
			args.push(content);
			return run(args, { json: true, cwd });
		},

		async search({ query, namespace, limit, match }, cwd) {
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
			return run(args, { json: true, cwd });
		},

		async list({ namespace, limit, full }, cwd) {
			const args = ["list"];
			if (namespace) {
				args.push("--namespace", namespace);
			}
			if (typeof limit === "number") {
				args.push("--limit", String(limit));
			}
			if (full === true) {
				args.push("--full");
			}
			return run(args, { json: true, cwd });
		},

		async sessionStart({ sessionId, namespace, directory }, cwd) {
			const args = ["session", "--id", sessionId];
			if (namespace) {
				args.push("--namespace", namespace);
			}
			if (directory) {
				args.push("--directory", directory);
			}
			return run(args, { json: true, cwd });
		},

		async sessionEnd({ sessionId, summary }, cwd) {
			const args = ["end", "--id", sessionId];
			if (summary) {
				args.push("--summary", summary);
			}
			return run(args, { json: true, cwd });
		},

		async savePrompt({ content, sessionId, namespace }, cwd) {
			const args = ["prompt", content];
			if (sessionId) {
				args.push("--session", sessionId);
			}
			if (namespace) {
				args.push("--namespace", namespace);
			}
			return run(args, { json: true, cwd });
		},

		async searchPrompts({ query, matchMode, limit }, cwd) {
			const args = ["prompts", "--search", query];
			if (matchMode) {
				args.push("--match", matchMode);
			}
			if (typeof limit === "number") {
				args.push("--limit", String(limit));
			}
			return run(args, { json: true, cwd });
		},

		async suggestTopicKey({ content, namespace, limit }, cwd) {
			const args = ["suggest-topic-key", content];
			if (namespace) {
				args.push("--namespace", namespace);
			}
			if (typeof limit === "number") {
				args.push("--limit", String(limit));
			}
			return run(args, { json: true, cwd });
		},

		async show(uuid, cwd) {
			return run(["show", uuid], { json: true, cwd });
		},

		async supersede({ older, newer, reason }, cwd) {
			return run(["supersede", "--older", older, "--newer", newer, "--reason", reason], { cwd });
		},

		/** Git helpers: run a git command inside the workspace. */
		async git(args, cwd) {
			const result = await exec("git", args, { cwd });
			return {
				...result,
				value: result.code === 0 ? result.stdout.trim() : null,
			};
		},
	};
}
