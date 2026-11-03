// --- what a session is (odd/tasks/opencode-parity.md, S3) -------------------
//
// Moved out of `createCore` by the reorganization. The maps (`sessions`,
// `evidence`) and the journal stay the instance's: these functions only act on
// them.

/**
 * The three things a session is to this plugin: registered once, closed on the
 * host's own end signal, and captured message by message.
 *
 * `sessions`, `evidence` and `report` are the instance's, minted by
 * `createCore`; `cwd` is where the host placed it and `cli` is its client.
 */
function createSession({ sessions, evidence, cwd, cli, env, report }) {
	/** Register a session once, in the directory the host gave. */
	async function startSession(sessionID, directory) {
		if (typeof sessionID !== "string" || sessionID === "") {
			return;
		}
		if (sessions.has(sessionID)) {
			return;
		}
		const where = directory || cwd;
		sessions.set(sessionID, where);
		try {
			const result = await cli.sessionStart({ sessionId: sessionID, directory: where }, cwd);
			if (result.code !== 0) {
				// A profile that is not initialised, or a namespace not allowed
				// for writes, is the engine's answer; the session still works.
				report("session-start", { session: sessionID, exit_code: result.code });
				return;
			}
			const session = result.data && typeof result.data === "object" ? result.data : null;
			const endedAt =
				session && typeof session.ended_at === "string" && session.ended_at !== ""
					? session.ended_at
					: null;
			if (endedAt !== null) {
				// The engine's state machine, verified against the binary: a
				// second start of the same id is a no-op, and a start after an
				// end answers the ended row — it never reopens a session. So a
				// taken start is not an error, an ended id is not an error, and
				// neither is hidden from the journal.
				report("session-start", {
					session: sessionID,
					already_ended: true,
					ended_at: endedAt,
				});
			}
		} catch (error) {
			report("session-start", { session: sessionID, error: String(error) });
		}
	}

	/**
	 * Close one session the plugin opened: end it, then leave the workspace's one
	 * "estado al cierre" record for the next session to resume from.
	 *
	 * Both steps share one budget (R1): a host awaits this, so a store that does
	 * not answer is abandoned instead of holding the turn. The record goes through
	 * the same `run()` path as every other write, so the write budget and the busy
	 * retry apply. An abandoned or refused close keeps the session tracked, so the
	 * next close signal retries it — the end is idempotent and the record's title
	 * is its identity, so the retry revises both instead of adding a second answer.
	 */
	async function closeSession(sessionID) {
		const close = {
			session: sessionID,
			action: "skipped",
			steps: [],
			abandoned: [],
			namespace: null,
		};
		if (typeof sessionID !== "string" || sessionID === "") {
			close.reason = "the host named no session";
			return close;
		}
		if (!sessions.has(sessionID)) {
			// Only what the plugin opened is its to close.
			close.reason = "not opened by this plugin";
			return close;
		}
		const where = sessions.get(sessionID) ?? cwd;
		const budget = createDeadline(
			budgetOf(env[SHUTDOWN_BUDGET_ENV], SHUTDOWN_BUDGET_MS),
			"shutdown",
		);
		const journal = (data) =>
			report("session-end", {
				session: sessionID,
				action: close.action,
				steps: [...close.steps],
				namespace: close.namespace,
				ended_at: close.ended_at,
				...data,
			});
		const abandon = (step, reason) => {
			close.action = "abandoned";
			close.abandoned.push(reason);
			journal({ step, reason });
			return close;
		};
		try {
			const endStep = await budget.step("end", (left) =>
				cli.withBudget(left).sessionEnd({ sessionId: sessionID }, where),
			);
			if (endStep.abandoned) {
				return abandon("end", endStep.abandoned);
			}
			const ended = endStep.value;
			close.steps.push("end");
			close.exit_code = ended.code;
			if (ended.code === 3) {
				// The engine never heard of this session: nothing to end, and
				// nothing to write its state into. The refusal gets its name.
				close.action = "failed";
				close.not_registered = true;
				sessions.delete(sessionID);
				evidence.delete(sessionID);
				journal({ step: "end", exit_code: 3, not_registered: true });
				return close;
			}
			if (ended.code !== 0) {
				close.action = "failed";
				journal({ step: "end", exit_code: ended.code });
				return close;
			}
			const session = ended.data && typeof ended.data === "object" ? ended.data : null;
			const namespace =
				session && typeof session.namespace === "string" ? session.namespace : "";
			close.ended_at =
				session && typeof session.ended_at === "string" && session.ended_at !== ""
					? session.ended_at
					: null;
			if (namespace === "") {
				// The engine owns the workspace's namespace; a close that would have
				// to guess it is not a close, and says so.
				close.action = "failed";
				close.error = "the engine ended the session but answered no namespace";
				journal({ step: "end" });
				return close;
			}
			close.namespace = namespace;
			const saved = await budget.step("save", (left) =>
				cli.withBudget(left).saveMemory(
					{
						namespace,
						title: SESSION_CLOSE_TITLE,
						type: "project_state",
						content: sessionCloseContent({ id: sessionID, ...(session ?? {}) }),
					},
					where,
				),
			);
			if (saved.abandoned) {
				return abandon("save", saved.abandoned);
			}
			close.steps.push("save");
			close.exit_code = saved.value.code;
			if (saved.value.code !== 0) {
				// The session is over either way; the record is the part the next
				// close signal can still revise.
				close.action = "partial";
				journal({ step: "save", exit_code: saved.value.code });
				return close;
			}
			close.action = "closed";
			sessions.delete(sessionID);
			evidence.delete(sessionID);
			return close;
		} catch (error) {
			// The session is going away either way; a close that cannot reach the
			// engine is a report, never an exception in a host hook.
			close.action = "failed";
			close.error = String(error);
			journal({ step: "hook" });
			return close;
		}
	}

	/**
	 * Store what the person typed, byte for byte.
	 *
	 * The session is registered first, so the prompt joins the session and
	 * the memories of one workspace. A message the store refuses is never
	 * truncated and never lost silently: its size and exit code reach the
	 * journal, and its text never does. The capture runs on every message, so
	 * it gets a budget of its own: a store that cannot take the message inside
	 * it loses the message, never the turn.
	 */
	async function capture(sessionID, text) {
		if (typeof text !== "string" || text.trim() === "") {
			return;
		}
		await startSession(sessionID, cwd);
		const bytes = Buffer.byteLength(text, "utf8");
		const budget = createDeadline(
			budgetOf(env[CAPTURE_BUDGET_ENV], PROMPT_CAPTURE_BUDGET_MS),
			"prompt capture",
		);
		try {
			const captured = await budget.step("capture", (left) =>
				cli.withBudget(left).savePrompt({ content: text, sessionId: sessionID }, cwd),
			);
			if (captured.abandoned) {
				report("capture", { session: sessionID, bytes, abandoned: captured.abandoned });
				return;
			}
			const result = captured.value;
			if (result.code !== 0) {
				report("capture", {
					session: sessionID,
					bytes,
					exit_code: result.code,
					busy_retries: result.busy_retries,
				});
			}
		} catch (error) {
			report("capture", { session: sessionID, bytes, error: String(error) });
		}
	}
	return { startSession, closeSession, capture };
}
