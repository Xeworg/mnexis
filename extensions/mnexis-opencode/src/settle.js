// --- the session's own outcomes (odd/tasks/opencode-parity.md, S3) ----------
//
// Moved out of `createCore` by the reorganization: each lifetime of one plugin
// instance lives in its own section and takes the state the instance minted.

/**
 * What the session's own tool outcomes say, and the question they may earn.
 *
 * The evidence is the outcome, never the attempt: a tool that did not run, or
 * ran and failed, changed no file and saved nothing. `evidence` is the
 * instance's map, minted by `createCore` and read by the close.
 */
function createSettle({ evidence }) {
	function evidenceOf(sessionID) {
		let state = evidence.get(sessionID);
		if (!state) {
			state = { files: false, saves: 0, asked: false };
			evidence.set(sessionID, state);
		}
		return state;
	}

	/**
	 * One tool outcome, as both hosts report it.
	 *
	 * The evidence is the outcome, never the attempt: a tool that did not run,
	 * or ran and failed, changed no file and saved nothing. Both hosts name a
	 * failure in one of the fields below, and a call that names none of them
	 * is a call that landed.
	 */
	function observeTool(event) {
		if (!event || typeof event !== "object") {
			return;
		}
		const tool =
			typeof event.tool === "string"
				? event.tool
				: typeof event.toolName === "string"
					? event.toolName
					: "";
		if (tool === "") {
			return;
		}
		const failed =
			event.isError === true ||
			event.error != null ||
			event.result?.isError === true ||
			event.output?.isError === true;
		if (failed) {
			return;
		}
		const sessionID =
			typeof event.sessionID === "string" && event.sessionID !== ""
				? event.sessionID
				: typeof event.session_id === "string"
					? event.session_id
					: "";
		if (sessionID === "") {
			return;
		}
		const state = evidenceOf(sessionID);
		// A save that landed is the one datum that closes the question; a save
		// that was attempted and failed is not a save.
		if (tool === SAVE_TOOL_ID) {
			state.saves += 1;
			return;
		}
		// The gate already observes these same tools. A shell command is the
		// host's own signal that work happened, and the plugin cannot tell a
		// read from a write without spending a process on every command.
		if (SHELL_TOOLS.has(tool) || EDIT_TOOLS.has(tool)) {
			state.files = true;
		}
	}

	/**
	 * The settle question, once per session.
	 *
	 * It is asked when the session's own outcomes show work that may have
	 * changed a file and no save that landed. `asked` is the latch: one ask per
	 * session, whether or not the model takes it.
	 */
	function settle(sessionID) {
		if (typeof sessionID !== "string" || sessionID === "") {
			return null;
		}
		const state = evidence.get(sessionID);
		if (!state || !state.files || state.saves > 0 || state.asked) {
			return null;
		}
		state.asked = true;
		return SETTLE_NUDGE;
	}
	return { observeTool, settle };
}
