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

/** Human-readable, actionable message for a failed CLI call. */
export function describeFailure({ code, stderr, namespace }) {
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
		hint = "re-read the record with mnexis_show and retry with the current revision";
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
