// --- the behaviour every host shares --------------------------------------

/**
 * The plugin's engine-facing behaviour, shared by both plugin models.
 *
 * Every method is best effort: a host hook must never fail a turn because the
 * memory engine could not be reached, and it must never fail silently either —
 * a refusal reaches `logger`, which each adapter wires to whatever the host
 * offers (V1 has `client.app.log`; V2 has no logger, so its `ctx.storage` is
 * the journal — see `storageLogger`).
 */
function createCore({ cwd, client, env = process.env, logger, registry } = {}) {
	const children = registry ?? createChildRegistry();
	const cli = client ?? createClient({ env, registry: children });
	const sessions = new Map();
	const pointed = new Set();
	// The session's own outcomes: work that may have changed a file, and the
	// saves that landed. It is what the settle question is decided on.
	const evidence = new Map();

	function report(kind, data) {
		if (typeof logger !== "function") {
			return;
		}
		try {
			logger({ kind, ...data });
		} catch {
			// The journal is a report, never a precondition.
		}
	}

	// The three lifetimes of one instance, each in its own section: what the
	// session's outcomes say, what a session is, and what a session is told
	// once. The state above is the instance's, and it is all they share.
	const { observeTool, settle } = createSettle({ evidence });
	const { startSession, closeSession, capture } = createSession({ sessions, evidence, cwd, cli, env, report });
	const { pointer, compaction } = createPointer({ pointed, cwd, cli, env, report });

	/**
	 * The end of this plugin instance: the calls still in flight die first, so a
	 * capture cannot finish into a session that is going away. Nothing else runs
	 * here, and `RELOAD_SKIP_REASON` says why — a reload is not an end, and this
	 * host gives the unload no way to tell one from the other. The answer names
	 * what was skipped, so the decision is observable instead of implied.
	 */
	async function shutdown() {
		children.killAll();
		return {
			action: "skipped",
			reason: RELOAD_SKIP_REASON,
			steps: [],
			abandoned: [],
			sessions: [...sessions.keys()],
		};
	}

	return {
		cli,
		capture,
		startSession,
		closeSession,
		pointer,
		settle,
		observeTool,
		compaction,
		shutdown,
	};
}
