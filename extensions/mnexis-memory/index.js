/**
 * mnexis-memory — pi extension exposing the mnexis memory engine.
 *
 * Why an extension and not only a skill: a skill is advisory, so the model
 * decides whether to use it and which namespace to write into. These tools
 * are declared to the model in every session, the workspace → namespace
 * binding is resolved by the extension, and the write permission gate lives
 * in code instead of in prose.
 *
 * The gate asks the human exactly once per namespace. After that the agent
 * saves on its own, inside the namespaces the human already authorized.
 */

import { createClient, defaultExec } from "./lib/cli.js";
import { decideAuthorization, describeFailure, allowNamespaceCommand } from "./lib/gate.js";
import { deriveNamespaceName, selectNamespace, workspaceKey } from "./lib/workspace.js";

/** Read the write allowlist out of `mnexis config show --json`. */
export function readWriteNamespaces(configData) {
	const scope = configData && typeof configData === "object" ? configData.scope : null;
	const namespaces = scope && typeof scope === "object" ? scope.write_namespaces : null;
	return Array.isArray(namespaces) ? namespaces : [];
}

/**
 * Extract the records from a `--json` payload.
 *
 * `mnexis list --json` and `mnexis search --json` answer with an envelope
 * `{ count, items }`, not with a bare array.
 */
export function readItems(payload) {
	if (Array.isArray(payload)) {
		return payload;
	}
	if (payload && typeof payload === "object" && Array.isArray(payload.items)) {
		return payload.items;
	}
	return [];
}

/** Build the tool result shape pi expects. */
function ok(text, details) {
	return { content: [{ type: "text", text }], details };
}

/** Build a failed tool result that still carries structured data. */
function fail(text, details) {
	return { content: [{ type: "text", text }], details, isError: true };
}

/**
 * Commands and file writes that would grant write permissions without the
 * human. The extension owns that decision, so a model-issued call that
 * tries to do it behind the extension's back is blocked.
 */
const PERMISSION_GRANT_PATTERN =
	/\bmnexis\s+config\s+allow-namespace\b|\bmnexis\s+project\s+bind\b/;

/** True when a bash command would grant namespace permissions on its own. */
export function isPermissionGrantCommand(command) {
	if (typeof command !== "string") {
		return false;
	}
	return PERMISSION_GRANT_PATTERN.test(command);
}

/** True when a file write would edit the profile by hand. */
export function isProfileFilePath(path) {
	if (typeof path !== "string") {
		return false;
	}
	return /(?:^|[\\/])(mnexis|projects)\.toml$/.test(path);
}

/**
 * True when the profile settings file is missing and `mnexis init` must run
 * before anything else. `mnexis config show --json` answers with
 * `config_file_present`.
 */
export function needsProfileInit(configData) {
	if (!configData || typeof configData !== "object") {
		return true;
	}
	return configData.config_file_present === false;
}

export default function mnexisMemoryExtension(pi) {
	// One client and one resolution cache per extension instance (per session).
	const client = createClient({
		exec: defaultExec,
		binary: process.env.MNEXIS_BIN,
		env: process.env,
	});
	let resolution = null;
	let writeNamespaces = null;

	/** Resolve the workspace → namespace binding once per session. */
	async function resolveWorkspace(cwd) {
		if (resolution) {
			return resolution;
		}
		const root = await client.git(["rev-parse", "--show-toplevel"], cwd);
		const repoRoot = root.code === 0 && root.value ? root.value : null;
		const remote = await client.git(["config", "--get", "remote.origin.url"], repoRoot || cwd);
		const remoteUrl = remote.code === 0 && remote.value ? remote.value : null;
		const key = workspaceKey({ remote: remoteUrl, repoRoot, cwd });
		const rootPath = repoRoot || cwd;

		const list = await client.projectList(cwd);
		const bindings = list.code === 0 && list.data && Array.isArray(list.data.bindings) ? list.data.bindings : [];
		const fallbackName = deriveNamespaceName(rootPath);
		const selection = selectNamespace({ bindings, key, fallbackName });

		if (!selection.bound) {
			const bind = await client.bind(selection.namespace, key, rootPath, cwd);
			if (bind.code !== 0) {
				throw new Error(
					describeFailure({ code: bind.code, stderr: bind.stderr, namespace: selection.namespace }),
				);
			}
		}

		resolution = { key, namespace: selection.namespace, root: rootPath, remote: remoteUrl };
		return resolution;
	}

	/** Read the profile allowlist once per session, initializing it if needed. */
	async function getWriteNamespaces(cwd) {
		if (writeNamespaces) {
			return writeNamespaces;
		}
		let config = await client.configShow(cwd);
		if (config.code !== 0) {
			throw new Error(describeFailure({ code: config.code, stderr: config.stderr }));
		}
		// `mnexis project bind` works without a profile, so the binding can
		// exist while the profile does not. Every write needs the profile.
		if (needsProfileInit(config.data)) {
			const init = await client.init(cwd);
			if (init.code !== 0) {
				throw new Error(describeFailure({ code: init.code, stderr: init.stderr }));
			}
			config = await client.configShow(cwd);
			if (config.code !== 0) {
				throw new Error(describeFailure({ code: config.code, stderr: config.stderr }));
			}
		}
		writeNamespaces = readWriteNamespaces(config.data);
		return writeNamespaces;
	}

	/**
	 * Ensure the namespace may be written to, asking the human once.
	 *
	 * Interactive sessions confirm through pi's UI; headless sessions never
	 * authorize anything and report the exact command a human must run.
	 */
	async function ensureWritable(namespace, ctx) {
		const allowed = await getWriteNamespaces(ctx.cwd);
		const decision = decideAuthorization({
			namespace,
			writeNamespaces: allowed,
			hasUI: Boolean(ctx.hasUI),
			mode: ctx.mode,
		});
		if (decision.action === "allow") {
			return { authorized: true, asked: false };
		}
		if (decision.action === "pending") {
			return {
				authorized: false,
				asked: false,
				message:
					`Writing to namespace '${namespace}' needs your authorization first. ` +
					`Run \`${allowNamespaceCommand(namespace)}\` once, then retry. ` +
					`Reason: ${decision.reason}.`,
			};
		}
		const confirmed = await ctx.ui.confirm(
			`Allow the mnexis agent to write memories in namespace '${namespace}'?\n\n` +
				"This grants write permission (save, update, supersede) for that namespace only, " +
				"in the current mnexis profile. Other namespaces and global access stay untouched, " +
				"and you can revoke it later by editing the profile allowlist.",
			"mnexis: authorize namespace",
		);
		if (!confirmed) {
			return {
				authorized: false,
				asked: true,
				message: `Authorization for namespace '${namespace}' was declined. No memory was written.`,
			};
		}
		const allow = await client.allowNamespace(namespace, ctx.cwd);
		if (allow.code !== 0) {
			return {
				authorized: false,
				asked: true,
				message: describeFailure({ code: allow.code, stderr: allow.stderr, namespace }),
			};
		}
		writeNamespaces = [...allowed, namespace];
		return { authorized: true, asked: true };
	}

	/** Shared prologue for the write tools. */
	async function prepareWrite(ctx) {
		const workspace = await resolveWorkspace(ctx.cwd);
		const gate = await ensureWritable(workspace.namespace, ctx);
		if (!gate.authorized) {
			return { workspace, gate };
		}
		return { workspace, gate };
	}

	// The gate lives in this extension, so a model-issued call must not
	// grant the permissions behind its back. The human's own commands are
	// untouched: only model tool calls are inspected here.
	pi.on("tool_call", async (event) => {
		if (event.toolName === "bash" && isPermissionGrantCommand(event.input?.command)) {
			return {
				block: true,
				reason:
					"Granting a namespace is the human's decision. Use the mnexis tools " +
					"(they ask for confirmation once per namespace) or ask the human to run " +
					"`mnexis config allow-namespace <ns>` themselves.",
			};
		}
		if ((event.toolName === "write" || event.toolName === "edit") && isProfileFilePath(event.input?.path)) {
			return {
				block: true,
				reason:
					"The mnexis profile files are managed by the mnexis CLI, never edited by hand. " +
					"Use `mnexis init`, `mnexis project bind` or `mnexis config allow-namespace` instead.",
			};
		}
		return undefined;
	});

	pi.registerTool({
		name: "mnexis_save",
		label: "Save a project memory",
		promptSnippet: "Store a durable memory of this project in the local mnexis engine.",
		description:
			"Store a durable memory about this project so future sessions can recall it. " +
			"Use it after a decision, a discovery, a milestone or a stated preference — " +
			"content must make sense months later without the surrounding conversation. " +
			"The namespace is resolved from the current workspace automatically.",
		parameters: {
			type: "object",
			properties: {
				content: { type: "string", description: "One self-contained sentence." },
				tags: { type: "string", description: "Comma-separated tags." },
				topic_key: { type: "string", description: "Stable key grouping facts about one topic." },
				idempotency_key: { type: "string", description: "Makes retries safe." },
			},
			required: ["content"],
			additionalProperties: false,
		},
		async execute(toolCallId, params, signal, onUpdate, ctx) {
			const { workspace, gate } = await prepareWrite(ctx);
			if (!gate.authorized) {
				return fail(gate.message, { namespace: workspace.namespace, authorized: false });
			}
			const result = await client.save(
				{
					namespace: workspace.namespace,
					content: params.content,
					tags: params.tags,
					topicKey: params.topic_key,
					idempotencyKey: params.idempotency_key,
				},
				ctx.cwd,
			);
			if (result.code !== 0) {
				return fail(
					describeFailure({ code: result.code, stderr: result.stderr, namespace: workspace.namespace }),
					{ namespace: workspace.namespace },
				);
			}
			return ok(
				`Saved a memory in namespace '${workspace.namespace}'.`,
				{
					namespace: workspace.namespace,
					uuid: result.data && result.data.uuid ? result.data.uuid : null,
					output: result.stdout.trim(),
					data: result.data,
				},
			);
		},
	});

	pi.registerTool({
		name: "mnexis_search",
		label: "Search project memories",
		promptSnippet: "Recall what was recorded about this project.",
		description:
			"Search stored memories of this project (full-text, ranked). Use it before answering " +
			"from scratch when prior context may exist, and to check what was already recorded.",
		parameters: {
			type: "object",
			properties: {
				query: { type: "string", description: "Search terms." },
				limit: { type: "number", description: "Maximum number of hits (default: 10)." },
				namespace: { type: "string", description: "Optional other namespace to search." },
			},
			required: ["query"],
			additionalProperties: false,
		},
		async execute(toolCallId, params, signal, onUpdate, ctx) {
			const workspace = await resolveWorkspace(ctx.cwd);
			const result = await client.search(
				{ query: params.query, namespace: params.namespace || workspace.namespace, limit: params.limit },
				ctx.cwd,
			);
			if (result.code !== 0) {
				return fail(describeFailure({ code: result.code, stderr: result.stderr }), {
					namespace: params.namespace || workspace.namespace,
				});
			}
			const hits = readItems(result.data);
			const text = hits.length
				? hits
						.map((hit, index) => `${index + 1}. ${JSON.stringify(hit)}`)
						.join("\n")
				: `No memories matched '${params.query ?? ""}'.`;
			return ok(text, { namespace: params.namespace || workspace.namespace, count: hits.length, hits });
		},
	});

	pi.registerTool({
		name: "mnexis_list",
		label: "List project memories",
		promptSnippet: "List the memories recorded for this project.",
		description:
			"List the memories stored for this project, newest first. Use it to review what is " +
			"already recorded before saving something new.",
		parameters: {
			type: "object",
			properties: {
				namespace: { type: "string", description: "Optional other namespace to list." },
				limit: { type: "number", description: "Maximum number of memories." },
			},
			additionalProperties: false,
		},
		async execute(toolCallId, params, signal, onUpdate, ctx) {
			const workspace = await resolveWorkspace(ctx.cwd);
			const result = await client.list(
				{ namespace: params.namespace || workspace.namespace, limit: params.limit },
				ctx.cwd,
			);
			if (result.code !== 0) {
				return fail(describeFailure({ code: result.code, stderr: result.stderr }), {
					namespace: params.namespace || workspace.namespace,
				});
			}
			const items = readItems(result.data);
			const text = items.length
				? items.map((item, index) => `${index + 1}. ${JSON.stringify(item)}`).join("\n")
				: "No memories stored in this namespace yet.";
			return ok(text, { namespace: params.namespace || workspace.namespace, count: items.length, memories: items });
		},
	});

	pi.registerTool({
		name: "mnexis_show",
		label: "Show a memory",
		promptSnippet: "Read one stored memory by UUID.",
		description: "Read one memory by its UUID, including its current revision and state.",
		parameters: {
			type: "object",
			properties: {
				uuid: { type: "string", description: "Memory UUID." },
			},
			required: ["uuid"],
			additionalProperties: false,
		},
		async execute(toolCallId, params, signal, onUpdate, ctx) {
			const result = await client.show(params.uuid, ctx.cwd);
			if (result.code !== 0) {
				return fail(describeFailure({ code: result.code, stderr: result.stderr }), { uuid: params.uuid });
			}
			return ok(result.stdout.trim() || JSON.stringify(result.data), { uuid: params.uuid, memory: result.data });
		},
	});

	pi.registerTool({
		name: "mnexis_supersede",
		label: "Supersede a memory",
		promptSnippet: "Replace an outdated memory with a newer one, keeping the history.",
		description:
			"Replace an outdated memory with a newer one, keeping the history. Use it when a fact " +
			"changed: save the new fact first, then link both records with this tool.",
		parameters: {
			type: "object",
			properties: {
				older: { type: "string", description: "UUID of the outdated memory." },
				newer: { type: "string", description: "UUID of the replacing memory." },
				reason: { type: "string", description: "Why the newer fact replaces the older one." },
			},
			required: ["older", "newer", "reason"],
			additionalProperties: false,
		},
		async execute(toolCallId, params, signal, onUpdate, ctx) {
			const { workspace, gate } = await prepareWrite(ctx);
			if (!gate.authorized) {
				return fail(gate.message, { namespace: workspace.namespace, authorized: false });
			}
			const result = await client.supersede(
				{ older: params.older, newer: params.newer, reason: params.reason },
				ctx.cwd,
			);
			if (result.code !== 0) {
				return fail(
					describeFailure({ code: result.code, stderr: result.stderr, namespace: workspace.namespace }),
					{ namespace: workspace.namespace },
				);
			}
			return ok(`Superseded ${params.older} with ${params.newer}.`, {
				namespace: workspace.namespace,
				output: result.stdout.trim(),
			});
		},
	});
}
