/**
 * Render the engine's live state records as the block a compaction keeps.
 *
 * The selection is the engine's, never the plugin's: `list` already answers
 * the live `project_state` and `decision` records of this workspace's
 * namespace, newest first, and `--full` gives their whole content. The block
 * carries as many as fit its budget and says how many did not.
 *
 * @param {unknown} listData the `list --json` payload
 * @returns {string} the block, or "" when there is nothing to carry
 */
export function stateBlockText(listData) {
	const items = listData && typeof listData === "object" && Array.isArray(listData.items)
		? listData.items
		: [];
	const lines = [
		"mnexis — the live state of this project, which the summary above does not replace " +
			"(from the memory engine, newest first):",
	];
	let used = lines[0].length;
	let carried = 0;
	let kept = 0;
	for (const item of items) {
		if (!item || typeof item !== "object") continue;
		const content = typeof item.content === "string" ? item.content.trim() : "";
		if (content === "") continue;
		kept += 1;
		const type = typeof item.memory_type === "string" && item.memory_type !== "" ? item.memory_type : "fact";
		const title = typeof item.title === "string" && item.title !== "" ? item.title : "(untitled)";
		const uuid = typeof item.uuid === "string" ? item.uuid : "?";
		const revision = Number.isFinite(item.revision) ? `, rev ${item.revision}` : "";
		const head = `\n- [${type}] ${title} (${uuid}${revision})`;
		const body = `\n  ${content.split("\n").join("\n  ")}`;
		if (used + head.length + body.length > STATE_CHARS) continue;
		lines.push(head + body);
		used += head.length + body.length;
		carried += 1;
	}
	if (carried === 0) {
		return "";
	}
	const left = kept - carried;
	if (left > 0) {
		lines.push(
			`\n… ${left} more live record${left === 1 ? "" : "s"} did not fit; ` +
				"read them with `mnexis show <uuid>`.",
		);
	}
	return lines.join("");
}

/**
 * The one-shot pointer to the door.
 *
 * It names the namespace, the door and the rules that make the door cheap —
 * and it does **not** re-render the memories or the state block: the board
 * measured that an injected state block did not remove the verification
 * round (`odd/next.md` §11), so a second projection would buy tokens and
 * nothing else.
 *
 * @param {{ status?: unknown, previous?: unknown }} input
 *   `status`: the `status --json` payload; `previous`: the `recall
 *   --previous --json` payload.
 * @returns {string} the pointer, or "" when the workspace names no namespace
 */
export function pointerText({ status, previous } = {}) {
	const workspace = status && typeof status === "object" ? status.workspace : null;
	const namespace =
		workspace && typeof workspace.namespace === "string" && workspace.namespace !== ""
			? workspace.namespace
			: null;
	// A workspace with no binding has no namespace to name, and the plugin
	// never invents one: the skill teaches the binding, the human decides it.
	if (!namespace) {
		return "";
	}
	const count =
		status && typeof status === "object" && Number.isFinite(status.memory_count)
			? status.memory_count
			: null;
	const lines = [
		`mnexis memory: this workspace writes into namespace \`${namespace}\`` +
			`${count === null ? "" : ` (${count} memories)`}.`,
		"Call the door — `mnexis_memory_context`, or `tools.mnexis.memory_context` under " +
			"Code Mode — with the task in one line before you read or change code: it answers " +
			"the matching memories, the live project state and the previous session's summary.",
	];
	const summary = previousSummary(previous);
	if (summary) {
		lines.push(`Previous session (${summary.when}): ${summary.text}`);
	}
	lines.push(
		"What the user says is already stored verbatim: never copy the conversation into " +
			"`memory_save_prompt`.",
	);
	return lines.join("\n");
}

/**
 * The mechanical state a closed session leaves in the engine.
 *
 * Code can only state what the engine answered: the session id, its boundaries,
 * the directory it ran in and the branch it started from. The summary is the
 * narrative and the engine's session row already carries it, so the record does
 * not duplicate it. Nothing is invented, and a fact the engine did not answer is
 * omitted instead of written as `undefined`. The shape is pi's
 * (`extensions/mnexis-memory/index.ts`), because the record is the engine's.
 *
 * @param {unknown} session the `end --json` payload
 * @returns {string}
 */
export function sessionCloseContent(session) {
	const data = session && typeof session === "object" ? session : {};
	const text = (value) => (typeof value === "string" ? value.trim() : "");
	const id = text(data.id) || "unknown";
	const startedAt = text(data.started_at);
	const endedAt = text(data.ended_at);
	const directory = text(data.directory);
	const branch = text(data.branch);
	const branchSha = text(data.branch_sha);
	const branchRemote = text(data.branch_remote);

	const lines = [`Session ${id} ended${endedAt ? ` at ${endedAt}` : ""}.`];
	if (startedAt) {
		lines.push(`It started at ${startedAt}.`);
	}
	if (directory) {
		lines.push(`Directory: ${directory}.`);
	}
	if (branch) {
		const at = branchSha ? ` at ${branchSha}` : "";
		const remote = branchRemote ? ` (${branchRemote})` : "";
		lines.push(`Branch: ${branch}${at}${remote}.`);
	}
	return lines.join("\n");
}

/**
 * The same mechanical state, for a session that is still open.
 *
 * A host without an end-of-session event (opencode v2: it publishes the pause,
 * not the end) refreshes the record at every pause. The record is the same one
 * — the title is the identity, so the write revises it — but it must not claim
 * the session ended: it is open, and it paused. A record that says "ended"
 * while the session still runs is a lie the next session would read.
 *
 * @param {unknown} session the session row the engine answered
 * @returns {string}
 */
export function sessionPauseContent(session) {
	const data = session && typeof session === "object" ? session : {};
	const text = (value) => (typeof value === "string" ? value.trim() : "");
	const id = text(data.id) || "unknown";
	const startedAt = text(data.started_at);
	const directory = text(data.directory);
	const branch = text(data.branch);
	const branchSha = text(data.branch_sha);
	const branchRemote = text(data.branch_remote);

	const lines = [`Session ${id} is still open; it paused.`];
	if (startedAt) {
		lines.push(`It started at ${startedAt}.`);
	}
	if (directory) {
		lines.push(`Directory: ${directory}.`);
	}
	if (branch) {
		const at = branchSha ? ` at ${branchSha}` : "";
		const remote = branchRemote ? ` (${branchRemote})` : "";
		lines.push(`Branch: ${branch}${at}${remote}.`);
	}
	return lines.join("\n");
}

/** The previous session's summary, as one bounded line. */
function previousSummary(previous) {
	const session = previous && typeof previous === "object" ? previous.session : null;
	if (!session || typeof session !== "object") {
		return null;
	}
	const raw = typeof session.summary === "string" ? session.summary.trim() : "";
	if (raw === "") {
		return null;
	}
	const firstLine = raw.split("\n")[0].trim();
	const text =
		firstLine.length > SUMMARY_CHARS ? `${firstLine.slice(0, SUMMARY_CHARS - 1)}…` : firstLine;
	const started = typeof session.started_at === "string" ? session.started_at : "";
	const when = started.length >= 10 ? started.slice(0, 10) : "date unknown";
	return { when, text };
}
