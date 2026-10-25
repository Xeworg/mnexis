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
 * The scope a read was bounded by, as the engine declared it: `all`,
 * `except_global` or `restricted`.
 *
 * The engine answers every read with it, so the agent can tell "there are
 * no memories" from "you may not read them". An older engine does not
 * declare it, and then the answer says nothing.
 */
export function readScope(payload) {
	if (payload && typeof payload === "object" && typeof payload.read_scope === "string") {
		return payload.read_scope;
	}
	return "";
}

/**
 * One line telling the agent how bounded the answer was. Empty when the
 * engine did not say (an older engine, or a payload without the field).
 */
export function scopeLine(payload) {
	const scope = readScope(payload);
	if (!scope) {
		return "";
	}
	if (scope === "all") {
		return "Read scope: all (every namespace is readable).";
	}
	if (scope === "except_global") {
		return "Read scope: except_global (the reserved namespace is not readable).";
	}
	return "Read scope: restricted (this profile may not read every namespace).";
}

/**
 * The namespace the engine says it searched.
 *
 * The one-call door declares it, so an empty answer can be told apart
 * from an answer that looked in another project. An older engine does not
 * declare it, and then the answer says nothing.
 */
export function searchedNamespace(payload) {
	if (payload && typeof payload === "object" && typeof payload.searched_namespace === "string") {
		return payload.searched_namespace === "" ? null : payload.searched_namespace;
	}
	return null;
}

/**
 * The relations between the memories a payload holds: which one replaced
 * which.
 *
 * The answer says it in the same breath as the memories, so the agent does
 * not have to open every record to know which one is current.
 */
export function readRelations(payload) {
	if (payload && typeof payload === "object" && Array.isArray(payload.relations)) {
		return payload.relations;
	}
	return [];
}

/**
 * One line per relation, in the words the agent needs: "this one was
 * replaced by that one".
 */
export function relationLines(relations) {
	if (!Array.isArray(relations) || relations.length === 0) {
		return "";
	}
	return (
		"\nRelations: " +
		relations
			.map((r) => {
				const reason = r && r.reason ? ` (${r.reason})` : "";
				return `${r.older} was replaced by ${r.newer}${reason}`;
			})
			.join("; ")
	);
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

/**
 * The id of the session the extension runs in, when the host has one.
 *
 * The id belongs to the host: the agent never invents one, and the
 * extension only passes it along.
 */
export function sessionIdOf(ctx) {
	if (!ctx || !ctx.sessionManager) {
		return null;
	}
	try {
		const id = ctx.sessionManager.getSessionId();
		return typeof id === "string" && id !== "" ? id : null;
	} catch {
		return null;
	}
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

	// The session the extension runs in: the engine knows its boundaries
	// without the model having to remember them. Both hooks are best
	// effort — a session that was never registered is not an error, it is
	// a session the engine did not hear about.
	pi.on("session_start", async (_event, ctx) => {
		const id = sessionIdOf(ctx);
		if (!id) {
			return;
		}
		try {
			await client.sessionStart({ sessionId: id, directory: ctx.cwd }, ctx.cwd);
		} catch {
			// The profile may not be initialised yet, or the namespace may
			// not be allowed for writes. The session still works.
		}
	});

	pi.on("session_shutdown", async (_event, ctx) => {
		const id = sessionIdOf(ctx);
		if (!id) {
			return;
		}
		try {
			await client.sessionEnd({ sessionId: id }, ctx.cwd);
		} catch {
			// Same as above: the end of a session is not worth an error.
		}
	});

	// Where a compaction lands. The summary of the compacted context is
	// what the session was doing; storing it means the next session can
	// recover it from the memory instead of from a context that no longer
	// exists. Only where the human already allowed writes.
	pi.on("session_compact", async (event, ctx) => {
		const summary = event?.compactionEntry?.summary;
		if (typeof summary !== "string" || summary.trim() === "") {
			return;
		}
		try {
			const workspace = await resolveWorkspace(ctx.cwd);
			if (!workspace.namespace) {
				return;
			}
			const allowed = await getWriteNamespaces(ctx.cwd);
			if (!allowed.includes(workspace.namespace)) {
				return;
			}
			const id = sessionIdOf(ctx);
			await client.save(
				{
					namespace: workspace.namespace,
					content:
						`Session ${id ?? "unknown"} was compacted. What was happening:\n\n${summary}`,
					tags: ["compaction"],
				},
				ctx.cwd,
			);
		} catch {
			// A compaction must not fail because the memory could not take it.
		}
	});

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
			"The namespace is resolved from the current workspace automatically. " +
			"Give it a title: the title is what the fact IS, the content is what it says " +
			"now, and the title decides which record the memory lands on. The same title " +
			"always means the same record, so a fact that changed updates its record " +
			"instead of adding a second one. Give it keywords too: the words the same fact " +
			"is called by in another language or another team's jargon, so it is found " +
			"when it is asked for that way. They are the topic's vocabulary, never a value.",
		parameters: {
			type: "object",
			properties: {
				content: { type: "string", description: "One self-contained sentence." },
				title: {
					type: "string",
					description:
						"Short, stable label of the fact (what it IS, not what it says now). " +
						"The same title always lands on the same record.",
				},
				tags: { type: "string", description: "Comma-separated tags." },
				keywords: {
					type: "string",
					description:
						"Comma-separated words the fact is called by in another language or " +
						"another team's jargon (e.g. `puerto, port, listen port`). They are the " +
						"topic's vocabulary, never a value, and they accumulate across saves.",
				},
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
					title: params.title,
					tags: params.tags,
					keywords: params.keywords,
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
			// The same title lands on the same record, so a save can update
			// a fact instead of adding a second one. The model has to read
			// that it happened.
			const revision = result.data && result.data.revision;
			const topicKey = result.data && result.data.topic_key;
			const what =
				typeof revision === "number" && revision > 1
					? `Updated the memory in namespace '${workspace.namespace}' (revision ${revision}, topic key \`${topicKey}\`).`
					: `Saved a memory in namespace '${workspace.namespace}'.`;
			// The words the record keeps after the save, not the ones this call
			// named: the vocabulary accumulates, so the answer is the only place
			// that knows what the memory carries now.
			const words =
				result.data && Array.isArray(result.data.keywords) ? result.data.keywords : [];
			const whatWithWords =
				words.length > 0 ? `${what} Words: ${words.join(", ")}.` : what;
			return ok(
				whatWithWords,
				{
					namespace: workspace.namespace,
					uuid: result.data && result.data.uuid ? result.data.uuid : null,
					title: (result.data && result.data.title) || params.title || null,
					keywords: words,
					topic_key: topicKey || null,
					revision: revision || null,
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
			const relations = readRelations(result.data);
			const scope = scopeLine(result.data);
			const text = hits.length
				? hits
						.map((hit, index) => `${index + 1}. ${JSON.stringify(hit)}`)
						.join("\n")
				: `No memories matched '${params.query ?? ""}'.`;
			return ok(text + relationLines(relations) + (scope ? `\n${scope}` : ""), {
				namespace: namespace ?? null,
				count: hits.length,
				hits,
				relations,
				read_scope: readScope(result.data),
			});
		},
	});

	pi.registerTool({
		name: "mnexis_suggest_topic_key",
		label: "Suggest the topic key",
		promptSnippet: "Ask which topic key a fact is already recorded under.",
		description:
			"Ask which topic key a title yields, and whether this project already records " +
			"it. Use it before saving a fact you are not sure about: the same title always " +
			"lands on the same record, so a fact that changed updates its record instead of " +
			"adding a second one.",
		parameters: {
			type: "object",
			properties: {
				title: { type: "string", description: "The title of the fact you are about to record." },
				namespace: {
					type: "string",
					description: "Optional namespace to ask in; the workspace's one by default.",
				},
			},
			required: ["title"],
			additionalProperties: false,
		},
		async execute(toolCallId, params, signal, onUpdate, ctx) {
			// A read: it resolves the workspace namespace like the read-only
			// tools do — no write confirmation — so the answer is about the
			// project the agent is working in, not the profile's default
			// namespace (which the workspace guard refuses).
			const workspace = await resolveWorkspace(ctx.cwd);
			const namespace = readNamespace(params.namespace) ?? workspace.namespace;
			const result = await client.suggestTopicKey({ title: params.title, namespace }, ctx.cwd);
			if (result.code !== 0) {
				return fail(describeFailure({ code: result.code, stderr: result.stderr, namespace }), {
					namespace: namespace ?? null,
				});
			}
			const data = result.data || {};
			const key = data.topic_key || "";
			const held = data.recorded_by;
			const text = !key
				? "This title yields no topic key."
				: held
					? `The key \`${key}\` is already recorded by ${held}: saving with this title updates that record.`
					: `The key \`${key}\` is free.`;
			return ok(text, {
				title: params.title,
				namespace: namespace ?? null,
				topic_key: key,
				recorded_by: held || null,
			});
		},
	});

	pi.registerTool({
		name: "mnexis_list",
		label: "List memories",
		promptSnippet: "List the memories recorded, across projects.",
		description:
			"List the memories stored, newest first: what is there, with a preview " +
			"of each one. Use it to review what is already recorded before saving " +
			"something new. The list reaches every project by default; pass `namespace` " +
			"to narrow it to one project. Each item names the memory and shows a few " +
			"words of it: read the one that matters with `mnexis_show`, and ask for " +
			"`full` only when you need the whole text of every memory.",
		parameters: {
			type: "object",
			properties: {
				namespace: {
					type: "string",
					description: "Optional namespace to narrow the list to one project.",
				},
				limit: { type: "number", description: "Maximum number of memories." },
				full: {
					type: "boolean",
					description: "The complete content of every memory, not a preview.",
				},
			},
			additionalProperties: false,
		},
		async execute(toolCallId, params, signal, onUpdate, ctx) {
			const namespace = readNamespace(params.namespace);
			const result = await client.list(
				{ namespace, limit: params.limit, full: params.full },
				ctx.cwd,
			);
			if (result.code !== 0) {
				return fail(describeFailure({ code: result.code, stderr: result.stderr }), {
					namespace: namespace ?? null,
				});
			}
			const items = readItems(result.data);
			const relations = readRelations(result.data);
			const scope = scopeLine(result.data);
			const text = items.length
				? items.map((item, index) => `${index + 1}. ${JSON.stringify(item)}`).join("\n")
				: "No memories stored yet.";
			return ok(text + relationLines(relations) + (scope ? `\n${scope}` : ""), {
				namespace: namespace ?? null,
				count: items.length,
				memories: items,
				relations,
				read_scope: readScope(result.data),
			});
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
		name: "mnexis_history",
		label: "The history of a memory",
		promptSnippet: "Every revision of one memory, and why each happened.",
		description:
			"Return the revision history of one memory: every revision in order, with its state, " +
			"content, who changed it and why. Use it to see how a fact evolved before updating or " +
			"superseding it — the current record alone does not say what it used to claim.",
		parameters: {
			type: "object",
			properties: {
				uuid: { type: "string", description: "Memory UUID." },
				limit: {
					type: "number",
					description: "How many revisions to show, oldest first (default: all).",
				},
			},
			required: ["uuid"],
			additionalProperties: false,
		},
		async execute(toolCallId, params, signal, onUpdate, ctx) {
			const result = await client.history({ uuid: params.uuid, limit: params.limit }, ctx.cwd);
			if (result.code !== 0) {
				return fail(describeFailure({ code: result.code, stderr: result.stderr }), {
					uuid: params.uuid,
				});
			}
			const items = readItems(result.data);
			return ok(result.stdout.trim() || JSON.stringify(result.data), {
				uuid: params.uuid,
				count: items.length,
				items,
			});
		},
	});

	pi.registerTool({
		name: "mnexis_session_start",
		label: "Start the session",
		promptSnippet: "Register the session you are running in.",
		description:
			"Register the session you are running in, so the engine knows its boundaries. Call it " +
			"once, at the start of the session: the session id is yours (the one you were given), " +
			"never invented. The engine never ends a session for you.",
		parameters: {
			type: "object",
			properties: {
				session_id: { type: "string", description: "The session id you were given." },
				directory: { type: "string", description: "Where the session runs (informational)." },
			},
			additionalProperties: false,
		},
		async execute(toolCallId, params, signal, onUpdate, ctx) {
			const sessionId = params.session_id ?? sessionIdOf(ctx);
			if (!sessionId) {
				return fail("This session has no id to register: pass `session_id`.", {
					authorized: false,
				});
			}
			const result = await client.sessionStart(
				{ sessionId, directory: params.directory ?? ctx.cwd },
				ctx.cwd,
			);
			if (result.code !== 0) {
				return fail(describeFailure({ code: result.code, stderr: result.stderr }), {
					session_id: sessionId,
				});
			}
			return ok(`Registered the session ${sessionId}.`, {
				session_id: sessionId,
				session: result.data,
			});
		},
	});

	pi.registerTool({
		name: "mnexis_session_end",
		label: "End the session",
		promptSnippet: "End the session with a summary of what it did.",
		description:
			"End the session you are running in, with a short summary of what it did. The summary " +
			"is what the next session reads first, so it must make sense with no surrounding chat.",
		parameters: {
			type: "object",
			properties: {
				session_id: { type: "string", description: "The session id you were given." },
				summary: { type: "string", description: "What the session did, in a few sentences." },
			},
			additionalProperties: false,
		},
		async execute(toolCallId, params, signal, onUpdate, ctx) {
			const sessionId = params.session_id ?? sessionIdOf(ctx);
			if (!sessionId) {
				return fail("This session has no id to end: pass `session_id`.", {
					authorized: false,
				});
			}
			const result = await client.sessionEnd(
				{ sessionId, summary: params.summary },
				ctx.cwd,
			);
			if (result.code !== 0) {
				return fail(describeFailure({ code: result.code, stderr: result.stderr }), {
					session_id: sessionId,
				});
			}
			return ok(`Ended the session ${sessionId}.`, {
				session_id: sessionId,
				session: result.data,
			});
		},
	});

	pi.registerTool({
		name: "mnexis_save_prompt",
		label: "Save what the user said",
		promptSnippet: "Store what the user said, so it survives the session.",
		description:
			"Store what the user said, verbatim. Use it when the user states a constraint, a " +
			"preference or a goal that has to survive the session: the conversation is not in the " +
			"repository, and this is where it goes.",
		parameters: {
			type: "object",
			properties: {
				content: { type: "string", description: "What the user said, verbatim." },
				session_id: { type: "string", description: "The session it belongs to." },
			},
			required: ["content"],
			additionalProperties: false,
		},
		async execute(toolCallId, params, signal, onUpdate, ctx) {
			const result = await client.savePrompt(
				{ content: params.content, sessionId: params.session_id ?? sessionIdOf(ctx) },
				ctx.cwd,
			);
			if (result.code !== 0) {
				return fail(describeFailure({ code: result.code, stderr: result.stderr }), {
					authorized: false,
				});
			}
			return ok("Stored what the user said.", { prompt: result.data });
		},
	});

	pi.registerTool({
		name: "mnexis_search_prompts",
		label: "Search what the user said",
		promptSnippet: "Find what the user asked before, across sessions.",
		description:
			"Search what the user said before, across sessions. Use it when you need the request " +
			"behind the work: the prompt is not in the files. Write the query the way you would " +
			"say it: every word counts and a hyphen is part of a word.",
		parameters: {
			type: "object",
			properties: {
				query: { type: "string", description: "Search terms, in plain language." },
				match_mode: {
					type: "string",
					enum: ["any", "all", "phrase"],
					description:
						"How the words are matched: any (default) counts every word, all requires " +
						"every word, phrase keeps the query as one exact phrase.",
				},
				limit: { type: "number", description: "Maximum number of hits (default: 20)." },
			},
			required: ["query"],
			additionalProperties: false,
		},
		async execute(toolCallId, params, signal, onUpdate, ctx) {
			const result = await client.searchPrompts(
				{ query: params.query, matchMode: params.match_mode, limit: params.limit },
				ctx.cwd,
			);
			if (result.code !== 0) {
				return fail(describeFailure({ code: result.code, stderr: result.stderr }), {
					authorized: false,
				});
			}
			const items = readItems(result.data);
			const text = items.length
				? items.map((item, index) => `${index + 1}. ${JSON.stringify(item)}`).join("\n")
				: "No prompts stored yet.";
			return ok(text, { count: items.length, prompts: items });
		},
	});

	pi.registerTool({
		name: "mnexis_recall",
		label: "Recall a session",
		promptSnippet: "What a session was, in one answer.",
		description:
			"Recall what a session was: the session itself, what the user said in it in order, and " +
			"the memories of the namespace in its window. Use `previous` for the session before this " +
			"one — the most recent finished session of this workspace's namespace — or `session` for a " +
			"known id. The memories are the namespace's, read by time: the engine does not know which " +
			"session wrote which memory. Each memory is answered with its identity and a preview; pass " +
			"`full` for the whole records, and read one with `mnexis_show`. The answer also carries the " +
			"state of the branch the session was anchored to, validated at read time and never stored: " +
			"`merged`, `not_merged`, or `unknown` when it could not be checked.",
		parameters: {
			type: "object",
			properties: {
				session: { type: "string", description: "The session id to recall." },
				previous: {
					type: "boolean",
					description: "The session before this one (the most recent finished session of the workspace's namespace).",
				},
				full: {
					type: "boolean",
					description: "The complete content of every memory in the window, not a preview.",
				},
			},
			additionalProperties: false,
		},
		async execute(toolCallId, params, signal, onUpdate, ctx) {
			// Exactly one way to ask: the engine refuses both and neither, and
			// so does the tool before anything reaches the CLI.
			if (Boolean(params.session) === Boolean(params.previous)) {
				return fail(
					"Give `session` (a known id) or `previous` (the session before this one), exactly one.",
					{ session: params.session ?? null, previous: Boolean(params.previous) },
				);
			}
			const result = await client.recall(
				{ session: params.session, previous: params.previous, full: params.full },
				ctx.cwd,
			);
			if (result.code !== 0) {
				return fail(describeFailure({ code: result.code, stderr: result.stderr }), {
					session: params.session ?? null,
					previous: Boolean(params.previous),
				});
			}
			return ok(result.stdout.trim() || JSON.stringify(result.data), {
				session_id:
					(result.data && result.data.session && result.data.session.id) ||
					params.session ||
					null,
				memories: result.data && Array.isArray(result.data.memories_in_window)
					? result.data.memories_in_window.length
					: 0,
				recall: result.data,
			});
		},
	});

	pi.registerTool({
		name: "mnexis_workspace",
		label: "This workspace, in one answer",
		promptSnippet: "Where this session may write, and what is already there.",
		description:
			"Call this FIRST, at the start of every session and before you change code that already " +
			"exists: it returns the namespace this workspace writes into, the read and write " +
			"allowlists, how many memories the scope can see, and the most recent of them — each " +
			"with its title and a preview, never the whole record (open one with mnexis_show when " +
			"it is the memory you need). The decisions behind the code live here, not in the files.",
		parameters: {
			type: "object",
			properties: {},
			additionalProperties: false,
		},
		async execute(toolCallId, params, signal, onUpdate, ctx) {
			const result = await client.status(ctx.cwd);
			if (result.code !== 0) {
				return fail(describeFailure({ code: result.code, stderr: result.stderr }), {});
			}
			return ok(result.stdout.trim() || JSON.stringify(result.data), {
				workspace: result.data && result.data.workspace ? result.data.workspace : null,
				status: result.data,
			});
		},
	});

	pi.registerTool({
		name: "mnexis_context",
		label: "The context of this task",
		promptSnippet: "Ask the project what it knows about this task, in one call.",
		description:
			"Ask the project a question in one call and get the budgeted answer: the " +
			"memories this project recorded that matter for the task, trimmed to a token " +
			"budget instead of the namespace and its rows. Use it as the FIRST call of a " +
			"session, before you read the code, and whenever the task is shaped like a " +
			"question ('where did we leave off', 'which port does it listen on', 'what did " +
			"we decide about X'). It searches this workspace's namespace — the one a save " +
			"lands in — and the answer declares the namespace it searched, the tokens it " +
			"spent and how far the read reached, so an empty answer is never another " +
			"project's silence. It does not replace mnexis_search: the search is the exact " +
			"query, the context is the project-shaped question.",
		parameters: {
			type: "object",
			properties: {
				task: { type: "string", description: "The task in one line." },
				namespace: {
					type: "string",
					description:
						"Optional namespace to ask another project. Defaults to this workspace's.",
				},
				token_budget: {
					type: "number",
					description: "Token budget for the answer (default: 1024).",
				},
				max_items: {
					type: "number",
					description: "Maximum number of memories to consider (default: 20, capped at 100).",
				},
				match: {
					type: "string",
					enum: ["any", "all", "phrase"],
					description:
						"How the task's words are matched: any (default) counts every word, all " +
						"requires every word, phrase keeps the task as one exact phrase.",
				},
			},
			required: ["task"],
			additionalProperties: false,
		},
		async execute(toolCallId, params, signal, onUpdate, ctx) {
			// The engine's own resolution of this workspace's namespace, read
			// from the status answer: a read, never a binding. The door must
			// not answer from another project, and the engine is the one that
			// decides which project this is.
			const status = await client.status(ctx.cwd);
			const engineNamespace =
				status.code === 0 &&
				status.data &&
				status.data.workspace &&
				typeof status.data.workspace.namespace === "string" &&
				status.data.workspace.namespace !== ""
					? status.data.workspace.namespace
					: null;
			const namespace = readNamespace(params.namespace) ?? engineNamespace ?? undefined;
			const result = await client.context(
				{
					task: params.task,
					namespace,
					tokenBudget: params.token_budget,
					maxItems: params.max_items,
					match: params.match,
				},
				ctx.cwd,
			);
			if (result.code !== 0) {
				return fail(describeFailure({ code: result.code, stderr: result.stderr, namespace }), {
					namespace: namespace ?? null,
				});
			}
			const items = readItems(result.data);
			// What the engine says it searched — not what we asked for: the
			// engine is the one that decided.
			const searched = searchedNamespace(result.data) ?? namespace ?? null;
			const scope = scopeLine(result.data);
			const tokens =
				result.data && typeof result.data.tokens_used === "number"
					? result.data.tokens_used
					: null;
			const text = items.length
				? items.map((item, index) => `${index + 1}. ${JSON.stringify(item)}`).join("\n")
				: `No memories answered the task '${params.task ?? ""}'.`;
			const declaration = [
				`Namespace searched: ${searched ?? "unknown"}.`,
				tokens === null ? "" : `Tokens: ${tokens}.`,
				scope,
			]
				.filter(Boolean)
				.join(" ");
			return ok(text + `\n${declaration}`, {
				namespace: namespace ?? null,
				searched_namespace: searched,
				count: items.length,
				tokens_used: tokens,
				memories: items,
				read_scope: readScope(result.data),
			});
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
