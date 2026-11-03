// --- what a session is told once (odd/tasks/opencode-parity.md, S3) ---------
//
// Moved out of `createCore` by the reorganization. Both members run on the
// model-request path, so both are bounded by a deadline of their own.

/**
 * The two projections this plugin puts in front of the model: the one-shot
 * pointer to the door, and the live state a compaction keeps.
 *
 * `pointed` is the instance's set of sessions already pointed at, minted by
 * `createCore`, so a second model request in one session reads nothing.
 */
function createPointer({ pointed, cwd, cli, env, report }) {
	/**
	 * The pointer for this session, delivered once.
	 *
	 * A failed read is remembered as delivered too: an engine that cannot
	 * answer must not cost a CLI call on every model request. The two reads
	 * share one budget, because this runs on the model-request path.
	 */
	async function pointer(sessionID) {
		if (env[POINTER_ENV] === "0") {
			return null;
		}
		// A request the host did not name belongs to no session, so there is
		// no "once" to count against it: the pointer waits for a real one.
		if (typeof sessionID !== "string" || sessionID === "") {
			return null;
		}
		if (pointed.has(sessionID)) {
			return null;
		}
		pointed.add(sessionID);
		const budget = createDeadline(budgetOf(env[POINTER_BUDGET_ENV], POINTER_BUDGET_MS), "pointer");
		try {
			const statusStep = await budget.step("status", (left) =>
				cli.withBudget(left).status(cwd),
			);
			if (statusStep.abandoned) {
				report("pointer", { session: sessionID, abandoned: statusStep.abandoned });
				return null;
			}
			const status = statusStep.value;
			if (status.code !== 0) {
				report("pointer", { session: sessionID, exit_code: status.code });
				return null;
			}
			let previous = null;
			try {
				const recallStep = await budget.step("recall", (left) =>
					cli.withBudget(left).previousSession(cwd),
				);
				if (!recallStep.abandoned && recallStep.value.code === 0) {
					previous = recallStep.value.data;
				}
			} catch {
				// No previous session is not an error: the pointer is shorter.
			}
			const text = pointerText({ status: status.data, previous });
			return text === "" ? null : text;
		} catch (error) {
			report("pointer", { session: sessionID, error: String(error) });
			return null;
		}
	}

	/** The block a compaction keeps: the engine's own live state. */
	async function compaction() {
		try {
			const result = await cli.stateRecords(cwd, STATE_LIMIT);
			if (result.code !== 0) {
				report("compaction", { exit_code: result.code });
				return null;
			}
			const text = stateBlockText(result.data);
			return text === "" ? null : text;
		} catch (error) {
			report("compaction", { error: String(error) });
			return null;
		}
	}
	return { pointer, compaction };
}
