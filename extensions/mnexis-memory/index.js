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
import { findMarkerNamespace, selectNamespace, workspaceKey } from "./lib/workspace.js";

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

/**
 * The namespace a read tool targets.
 *
 * Reads are open across projects: without an explicit namespace the tool
 * does not narrow the search, so the agent sees what other projects
 * recorded. An explicit namespace narrows it to that one project.
 *
 * @param {unknown} value
 * @returns {string | undefined}
 */
export function readNamespace(value) {
	const namespace = typeof value === "string" ? value.trim() : "";
	return namespace === "" ? undefined : namespace;
}

/**
 * Check that every memory a write touches lives in the workspace
 * namespace.
 *
 * A write that crosses a project boundary is refused before it reaches
 * the CLI: this workspace writes only into its own namespace. A memory
 * that cannot be read cannot be checked either, and a write whose target
 * is unknown is refused too.
 *
 * @returns {Promise<{ message: string, details: Record<string, unknown> } | null>}
 */
export async function checkProjectBoundary(client, uuids, namespace, cwd) {
	for (const uuid of uuids) {
		const result = await client.show(uuid, cwd);
		if (result.code !== 0) {
			return {
				message:
					`Cannot read the memory ${uuid} to check which project it belongs to, ` +
					`so nothing was changed. This workspace writes only into '${namespace}'.`,
				details: { uuid, namespace },
			};
		}
		const owner =
			result.data && typeof result.data === "object" ? result.data.namespace : undefined;
		if (owner !== namespace) {
			return {
				message:
					`The memory ${uuid} lives in namespace '${owner ?? "unknown"}' and this ` +
					`workspace writes only into '${namespace}'. Saving a memory for another project ` +
					`is the human's decision, not the agent's.`,
				details: { uuid, namespace: owner ?? null, workspace: namespace },
			};
		}
	}
	return null;
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
		// The engine owns the identity rule: the registry publishes the
		// canonical key of the workspace this process runs in, so the
		// binding the human recorded and the one the agent looks up are
		// the same key. The computed key stays as the fallback for an
		// older engine that does not publish it yet.
		const publishedKey =
			list.code === 0 && list.data && typeof list.data.current_key === "string" && list.data.current_key !== ""
				? list.data.current_key
				: null;
		const canonicalKey = publishedKey ?? key;
		// A `.mnexis-namespace` marker names the namespace of the
		// workspace; the mnexis CLI resolves it the same way, so both
		// sides agree on which project a directory belongs to.
		const marked = findMarkerNamespace(rootPath);
		const selection = selectNamespace({ bindings, key: canonicalKey, fallbackName: marked });

		if (!selection.bound && selection.namespace) {
			// The human wrote the namespace down (in the marker), so the
			// binding can be recorded. Nothing else is bound here: a
			// namespace the agent invented is a namespace nobody allowed.
			const bind = await client.bind(selection.namespace, canonicalKey, rootPath, cwd);
			if (bind.code !== 0) {
				throw new Error(
					describeFailure({ code: bind.code, stderr: bind.stderr, namespace: selection.namespace }),
				);
			}
		}

		resolution = { key: canonicalKey, namespace: selection.namespace, root: rootPath, remote: remoteUrl };
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
		if (!workspace.namespace) {
			// No binding and no marker: the namespace is the human's
			// decision, and the message has to say exactly what to run.
			return {
				workspace,
				gate: {
					authorized: false,
					asked: false,
					message:
						`This workspace (${workspace.key}) has no namespace yet, and the agent ` +
						`does not choose one. Ask the human to run:\n` +
						`  mnexis project bind <namespace> --key ${workspace.key}\n` +
						`  mnexis config allow-namespace <namespace>\n` +
						`and retry. Reading is still available.`,
				},
			};
		}
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
		label: "Search memories",
		promptSnippet: "Recall what was recorded, across projects.",
		description:
			"Search stored memories (full-text, ranked). Use it before answering " +
			"from scratch when prior context may exist, and to check what was already recorded. " +
			"The search reaches every project by default, because projects share components; " +
			"pass `namespace` to narrow it to one project. " +
			"Write the query the way you would say it: every word counts and a hyphen is " +
			"part of a word, so `staging-eu` and `puerto base de datos` both work.",
		parameters: {
			type: "object",
			properties: {
				query: { type: "string", description: "Search terms, in plain language." },
				limit: { type: "number", description: "Maximum number of hits (default: 10)." },
				namespace: {
					type: "string",
					description: "Optional namespace to narrow the search to one project.",
				},
				match: {
					type: "string",
					enum: ["any", "all", "phrase"],
					description:
						"How the words are matched: any (default) counts every word, all requires " +
						"every word, phrase keeps the query as one exact phrase.",
				},
			},
			required: ["query"],
			additionalProperties: false,
		},
		async execute(toolCallId, params, signal, onUpdate, ctx) {
			const namespace = readNamespace(params.namespace);
			const result = await client.search(
				{ query: params.query, namespace, limit: params.limit, match: params.match },
				ctx.cwd,
			);
			if (result.code !== 0) {
				return fail(describeFailure({ code: result.code, stderr: result.stderr }), {
					namespace: namespace ?? null,
				});
			}
			const hits = readItems(result.data);
			const text = hits.length
				? hits
						.map((hit, index) => `${index + 1}. ${JSON.stringify(hit)}`)
						.join("\n")
				: `No memories matched '${params.query ?? ""}'.`;
			return ok(text, { namespace: namespace ?? null, count: hits.length, hits });
		},
	});

	pi.registerTool({
		name: "mnexis_list",
		label: "List memories",
		promptSnippet: "List the memories recorded, across projects.",
		description:
			"List the memories stored, newest first. Use it to review what is " +
			"already recorded before saving something new. The list reaches every " +
			"project by default; pass `namespace` to narrow it to one project.",
		parameters: {
			type: "object",
			properties: {
				namespace: {
					type: "string",
					description: "Optional namespace to narrow the list to one project.",
				},
				limit: { type: "number", description: "Maximum number of memories." },
			},
			additionalProperties: false,
		},
		async execute(toolCallId, params, signal, onUpdate, ctx) {
			const namespace = readNamespace(params.namespace);
			const result = await client.list(
				{ namespace, limit: params.limit },
				ctx.cwd,
			);
			if (result.code !== 0) {
				return fail(describeFailure({ code: result.code, stderr: result.stderr }), {
					namespace: namespace ?? null,
				});
			}
			const items = readItems(result.data);
			const text = items.length
				? items.map((item, index) => `${index + 1}. ${JSON.stringify(item)}`).join("\n")
				: "No memories stored yet.";
			return ok(text, { namespace: namespace ?? null, count: items.length, memories: items });
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
			"changed: save the new fact first, then link both records with this tool. Both memories " +
			"must live in this project's namespace: the tool refuses to touch a memory that belongs " +
			"to another project.",
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
			const boundary = await checkProjectBoundary(
				client,
				[params.older, params.newer],
				workspace.namespace,
				ctx.cwd,
			);
			if (boundary) {
				return fail(boundary.message, {
					namespace: workspace.namespace,
					authorized: false,
					...boundary.details,
				});
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
