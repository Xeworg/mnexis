/**
 * Authorization gate for namespace writes.
 *
 * Permissions belong to the human, not to the agent. The gate is per
 * namespace and asks exactly once: once the namespace is authorized in the
 * profile, later writes are silent.
 */

/** Exit code / error meanings the agent can act on. */
export const EXIT_CODES = {
	0: "ok",
	1: "mnexis infrastructure failure",
	2: "invalid argument",
	3: "not found",
	4: "forbidden",
	5: "revision or topic conflict",
	6: "index unavailable",
	7: "budget too small",
	8: "cursor expired",
	9: "unsupported version",
	10: "busy",
};

/**
 * The exit-5 truth for the command that failed.
 *
 * Code 5 is `REVISION_CONFLICT` or `TOPIC_CONFLICT` (`src/cli/output.rs`), and
 * which of the two a caller can meet follows from what the caller sends: a
 * save carries no revision at all, a supersede resolves both of its revisions
 * itself, and `update` is the one command that takes the revision a retry must
 * name. A hint that promises a retry the caller cannot perform is worse than
 * none.
 */
function conflictHint(command) {
	const name = typeof command === "string" ? command : "";
	if (name === "save") {
		return (
			"the idempotency key was already used for a different payload, or another " +
			"writer claimed the topic key at the same moment: retry with the same payload " +
			"and the same `idempotency_key`, or with a new `idempotency_key`"
		);
	}
	if (name === "supersede") {
		return (
			"the older or the newer record changed while the supersede ran: read both " +
			"with the current `mnexis_show` answer and repeat the same supersede, which " +
			"resolves both revisions itself"
		);
	}
	if (name === "update") {
		return "re-read the record with mnexis_show and retry with the current revision";
	}
	return (
		"the record changed under the write (`REVISION_CONFLICT` or `TOPIC_CONFLICT`) " +
		"and nothing was written"
	);
}

/** Human-readable, actionable message for a failed CLI call. */
export function describeFailure({ code, stderr, namespace, command }) {
	const label = EXIT_CODES[code] ?? "unknown error";
	const tail = String(stderr ?? "")
		.trim()
		.split("\n")
		.slice(-3)
		.join(" ")
		.trim();
	let hint = "";
	if (code === 3) {
		hint = "the memory or namespace does not exist";
	} else if (code === 4) {
		hint = namespace
			? `the namespace '${namespace}' is not authorized for writes`
			: "the operation is not permitted";
	} else if (code === 5) {
		hint = conflictHint(command);
	}
	const parts = [`mnexis exited with code ${code} (${label})`];
	if (hint) {
		parts.push(hint);
	}
	if (tail) {
		parts.push(tail);
	}
	return parts.join(": ");
}

/**
 * Decide what to do about a write to `namespace`.
 *
 * @param {{ namespace: string, writeNamespaces: string[], hasUI: boolean, mode: string }} input
 * @returns {{ action: "allow" | "confirm" | "pending", reason?: string }}
 */
export function decideAuthorization({ namespace, writeNamespaces, hasUI, mode }) {
	const allowed = Array.isArray(writeNamespaces) ? writeNamespaces : [];
	if (allowed.includes(namespace)) {
		return { action: "allow" };
	}
	const interactive = Boolean(hasUI) && mode === "tui";
	if (!interactive) {
		return {
			action: "pending",
			reason: `namespace '${namespace}' is not authorized for writes and this session has no interactive UI to ask for consent`,
		};
	}
	return { action: "confirm" };
}

/** The exact command a human must run to authorize a namespace. */
export function allowNamespaceCommand(namespace) {
	return `mnexis config allow-namespace ${namespace}`;
}

/** The exact command a human must run to bind a workspace. */
export function bindCommand(namespace, key, path) {
	return `mnexis project bind ${namespace} --key ${key} --path ${path}`;
}

/**
 * Decide what to do about an unbound workspace that offers a candidate
 * namespace (auto-namespace S3).
 *
 * A candidate is a question, never a grant: the reserved `global` and every
 * name the engine refused are reported with their reason instead of being
 * offered (S4), and only an interactive session may ask the human (one
 * confirmation per workspace). A headless session gets the two commands to
 * hand over instead.
 *
 * @param {{ candidate?: string | null, refusal?: string | null, hasUI: boolean, mode: string }} input
 * @returns {{ action: "refuse", reason: string } | { action: "none" } | { action: "confirm" } | { action: "pending", reason: string }}
 */
export function decideBinding({ candidate, refusal, hasUI, mode }) {
	if (typeof refusal === "string" && refusal !== "") {
		return { action: "refuse", reason: refusal };
	}
	if (typeof candidate !== "string" || candidate === "") {
		return { action: "none" };
	}
	const interactive = Boolean(hasUI) && mode === "tui";
	if (!interactive) {
		return {
			action: "pending",
			reason:
				"this session has no interactive UI to ask for the workspace's namespace",
		};
	}
	return { action: "confirm" };
}
