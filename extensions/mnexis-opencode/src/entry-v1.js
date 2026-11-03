// --- opencode 1.x (odd/tasks/opencode-parity.md, S3) -----------------------

/**
 * The V1 entry: the plugin function returns one hooks object.
 *
 * `coreFor(cwd, logger)` builds one core for the instance; the rest is the
 * host's shape.
 */
function createV1Entry(coreFor) {
	async function server(input = {}) {
		const cwd =
			typeof input.directory === "string"
				? input.directory
				: typeof input.worktree === "string"
					? input.worktree
					: process.cwd();
		const hostLogger =
			input.client && input.client.app && typeof input.client.app.log === "function"
				? (record) =>
						void input.client.app.log({
							body: { service: PLUGIN_ID, level: "warn", message: "mnexis", extra: record },
						})
				: undefined;
		const core = coreFor(cwd, hostLogger);

		return {
			// The human's message, captured where it is said. A prompt a
			// plugin sent is `synthetic`, exactly like the host's own.
			async "chat.message"(hookInput, output) {
				if (!isHumanMessage(output?.message)) {
					return;
				}
				await core.capture(hookInput?.sessionID, promptTextFromParts(output?.parts));
			},

			async event({ event }) {
				if (!event) {
					return;
				}
				if (event.type === "session.created") {
					await core.startSession(sessionIDOfEvent(event), directoryOfEvent(event) ?? cwd);
					return;
				}
				if (event.type === "session.deleted") {
					// The host's own end signal: end the session and leave the state
					// it closed with. The answer is the close's own report.
					return core.closeSession(sessionIDOfEvent(event));
				}
			},

			async "tool.execute.before"(hookInput, output) {
				const refusal = gateViolation(hookInput?.tool, output?.args);
				if (refusal) {
					throw new Error(refusal);
				}
			},

			// The outcome of a tool call: the settle question's evidence.
			async "tool.execute.after"(hookInput, output) {
				core.observeTool({
					tool: hookInput?.tool,
					sessionID: hookInput?.sessionID,
					isError: output?.isError ?? hookInput?.isError,
					error: output?.error,
					result: output?.result,
				});
			},

			// The system prompt is assembled per model request. The pointer is
			// delivered once per session and is a no-op afterwards; the settle
			// question rides the same seat, once, when the session's own
			// outcomes show unsaved work.
			async "experimental.chat.system.transform"(hookInput, output) {
				if (!Array.isArray(output?.system)) {
					return;
				}
				const pointer = await core.pointer(hookInput?.sessionID);
				if (pointer) {
					output.system.push(pointer);
				}
				const settle = core.settle(hookInput?.sessionID);
				if (settle) {
					output.system.push(settle);
				}
			},

			async "experimental.session.compacting"(_hookInput, output) {
				const block = await core.compaction();
				if (block && Array.isArray(output?.context)) {
					output.context.push(block);
				}
			},

			// The host is going away: the sessions this plugin opened are ended for
			// real, with the state they leave behind. V1's own end signal —
			// `session.deleted` — does not fire in a run (measured live on 1.19.5,
			// 2026-11-03), so waiting for it left every V1 session open forever and
			// its closing record unwritten. The host still cannot say whether this
			// is a quit or a reload, and the engine tolerates a session ended
			// early: a prompt on it is still accepted.
			async dispose() {
				const sweep = await core.closeOpenSessions();
				const shutdown = await core.shutdown();
				return {
					...shutdown,
					action: sweep.action,
					closed: sweep.closed,
					abandoned: sweep.abandoned,
				};
			},
		};
	}

	return server;
}
