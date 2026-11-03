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
	// What a shell answers when the command does not exist, and what this
	// extension answers for a binary that was never installed: leaving it out
	// reported a missing engine as "exited with code 127 (unknown error)"
	// (S3, audit A8).
	127: "engine binary not found (exit 127, ENOENT)",
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

/**
 * A budget as the answer says it: `10 s`, `0.2 s`, or nothing when unknown.
 */
function seconds(ms) {
	const value = Number(ms);
	if (!Number.isFinite(value) || value <= 0) {
		return "";
	}
	return value % 1000 === 0 ? `${value / 1000} s` : `${Math.round(value / 10) / 100} s`;
}

/**
 * Human-readable, actionable message for a failed CLI call.
 *
 * The call's own name (`command`) and the budget it had (`timeout_ms`) ride in
 * the result, so a call the engine never answered says which one it was and
 * how long it was given — the audit's "a hung mnexis blocks every save" and
 * "127 (unknown error)" were failures of this one sentence (S2, S3).
 */
export function describeFailure({ code, stderr, namespace, command, timed_out, cancelled, timeout_ms }) {
	const name = command ? `mnexis ${command}` : "the mnexis call";
	if (cancelled) {
		return (
			`${name} was cancelled before it answered, so this call's result is unknown and nothing ` +
			"else was changed. Retry when the session is ready."
		);
	}
	if (timed_out) {
		const within = seconds(timeout_ms);
		return (
			`${name} did not answer${within ? ` within ${within}` : ""} and was killed, so this ` +
			"call's result is unknown. Retry; if it times out again the engine is stuck (check it " +
			"with `mnexis status`), or raise MNEXIS_TIMEOUT_MS if the store is only slow."
		);
	}
	if (code === 127) {
		return (
			`${name} could not run: ${EXIT_CODES[127]}, so nothing was read or written. Install the ` +
			"engine (`./install.sh`) or set MNEXIS_BIN to its path, then retry."
		);
	}
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
 * The answer for a call the engine did not answer at all (S3, audit A8).
 *
 * An engine that exits 0 having printed nothing, or printed something no JSON
 * reader can use, has not told the caller that the store is empty: it has told
 * the caller nothing. Reporting that silence as "No memories matched" is the
 * lie the audit measured, so the three sites read this sentence instead. A
 * write is a special case: the store may have taken the write before the
 * answer was lost, so the text says how to find out instead of inviting a
 * second record.
 *
 * @param {{ command?: string, no_answer?: string|null }} result
 * @param {{ wrote?: boolean }} options
 */
export function engineSilence({ command, no_answer } = {}, { wrote = false } = {}) {
	const name = command ? `mnexis ${command}` : "the mnexis call";
	const said =
		no_answer === "unreadable"
			? `\`${name}\` exited 0 with output this extension cannot read as JSON`
			: `\`${name}\` exited 0 and printed nothing`;
	const parts = [
		`The engine did not answer: ${said}, so this call failed and this is not an empty result.`,
	];
	if (wrote) {
		parts.push(
			"A write that was not acknowledged may still have landed: confirm with `mnexis_search` " +
				"before writing it again, or retry with the same idempotency key.",
		);
	} else {
		parts.push(`Retry, and run \`${name}\` in a terminal to read the engine's own error.`);
	}
	return parts.join(" ");
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
