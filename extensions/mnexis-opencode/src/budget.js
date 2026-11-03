// --- the budgets and the retry (odd/tasks/opencode-parity.md, S3) ---------
//
// Every call below runs inside a hook the host awaits, so each one carries a
// ceiling of its own. The numbers and the retry are the engine-compatible ones
// measured on the pi side before the split; this plugin keeps its own copy,
// because each host owns its logic and the engine is the only contract they
// share.

/** How long one engine read may take before the plugin stops it. */
export const READ_TIMEOUT_MS = 10_000;

/**
 * A write may wait on the store's lock and pay for its fsync, so it gets the
 * longer budget — thirty seconds, never forever.
 */
export const WRITE_TIMEOUT_MS = 30_000;

/**
 * The engine's exit code for a store that was locked (`src/cli/output.rs`).
 * The engine waits two seconds on the lock itself and then gives up; one more
 * try is what keeps a decided fact from being lost to another process's write.
 */
export const BUSY_EXIT_CODE = 10;

/** The waits between the tries of a call that found the store locked. */
export const BUSY_RETRY_DELAYS_MS = [250, 750];

/**
 * How long the capture of one message may take.
 *
 * The capture runs on every message, and the turn is the human's: a store that
 * cannot take the message in a second and a half loses the message, never the
 * turn.
 */
export const PROMPT_CAPTURE_BUDGET_MS = 1_500;

/**
 * How long the pointer's reads may take on the per-request path: the pointer
 * is a nicety, and a store that cannot answer it in a second and a half leaves
 * it out instead of delaying the model request.
 */
export const POINTER_BUDGET_MS = 1_500;

/** The environment override for the capture budget. */
export const CAPTURE_BUDGET_ENV = "MNEXIS_OPENCODE_CAPTURE_BUDGET_MS";

/** The environment override for the pointer budget. */
export const POINTER_BUDGET_ENV = "MNEXIS_OPENCODE_POINTER_BUDGET_MS";

/**
 * How long every step of one session close may take together.
 *
 * pi awaits its shutdown with no timeout and gives the close a deadline of its
 * own (R1, `extensions/mnexis-memory/lib/budget.ts`). Here the close is the work
 * a host hook awaits: the end, its busy retries and the estado al cierre. Five
 * seconds is already pathological for two local writes; past it the state is
 * worth less than the human's turn. An abandoned close is journaled and
 * re-runnable — the end is idempotent and the record is one title — so the next
 * close signal revises both.
 */
export const SHUTDOWN_BUDGET_MS = 5_000;

/** The environment override for the close's budget. */
export const SHUTDOWN_BUDGET_ENV = "MNEXIS_OPENCODE_SHUTDOWN_BUDGET_MS";

/**
 * Why the unload path closes nothing.
 *
 * Neither model's unload carries a reason: V2's `setup` returns a cleanup typed
 * `Cleanup = () => Promise<void> | void` (`@opencode/plugin`'s `plugin.d.ts`),
 * and V1's `dispose()` takes no argument (RESEARCH §1). V2 hot-reloads a watched
 * plugin directory and runs that cleanup on the way out (RESEARCH §3), so at the
 * unload a reload and a quit are the same event — and the engine's verified state
 * machine never reopens an ended session, so an end there would kill a session
 * the human is still working in. The close therefore runs on the host's own end
 * signal (`session.deleted`), and the unload only kills what is in flight.
 *
 * The cost, written down in the README: a quit that never deletes its session
 * leaves the engine's row live. The engine makes that harmless — a second start
 * of the same id is a no-op and a start after an end never reopens — and the
 * next session's own close is the record that survives.
 */
export const RELOAD_SKIP_REASON =
	"the unload carries no reason on this host: a reload and a quit look the same, " +
	"so no session is ended here";

/**
 * The canonical title of the state a closing session leaves behind.
 *
 * The title is the engine's identity (`key_from_title`), so every close lands on
 * the record the previous close left and bumps its revision instead of adding a
 * second answer to "where did the last session leave the project". The wording
 * and the shape are pi's (`SESSION_CLOSE_TITLE`, `sessionCloseContent`), because
 * the record is the engine's, not a host's.
 */
export const SESSION_CLOSE_TITLE = "estado al cierre";

/** The commands that change the store: the longer budget. */
const WRITE_COMMANDS = new Set(["session", "end", "prompt", "save"]);

/**
 * The commands a locked store is worth trying again, because a second try is
 * the same effect and never a second row: a session row is keyed by its id, an
 * end stamps that row, a prompt is one row of the session, and the plugin's one
 * `save` names its title (the engine's identity for the record). A read is left
 * out on purpose: an answer that never came is worth one report, not three.
 */
const RETRY_COMMANDS = new Set(["session", "end", "prompt", "save"]);

/** The save tool's id on this host: the engine's `memory_save`, namespaced. */
const SAVE_TOOL_ID = "mnexis_memory_save";

/** A positive integer environment override, or the default the code names. */
export function budgetOf(override, fallback) {
	const value = Number.parseInt(String(override ?? ""), 10);
	return Number.isFinite(value) && value > 0 ? value : fallback;
}

/** The token a step gets when it ran out of time: never a value it could return. */
const ABANDONED = Symbol("abandoned");

function sleep(ms) {
	return new Promise((resolve) => setTimeout(resolve, ms));
}

/**
 * A deadline, in milliseconds from now.
 *
 * `step` runs one piece of work with whatever is left and abandons what does
 * not fit instead of waiting for it. The answer is `{ value }` or `{
 * abandoned: reason }` — never a rejection the caller has to tell apart from an
 * engine failure.
 *
 * @param {number} budgetMs how long the whole piece of work may take
 * @param {string} label what the work is, for the reason an abandonment carries
 */
export function createDeadline(budgetMs, label) {
	const budget = Number.isFinite(budgetMs) && budgetMs > 0 ? budgetMs : 0;
	const deadline = Date.now() + budget;
	const name = typeof label === "string" && label !== "" ? label : "the work";

	return {
		/** What is left of the budget, in milliseconds, never below zero. */
		remaining() {
			return Math.max(0, deadline - Date.now());
		},

		async step(step, work) {
			const left = this.remaining();
			if (left <= 0) {
				return {
					abandoned: `${step} was abandoned: the ${budget} ms ${name} budget was already spent`,
				};
			}
			let timer = null;
			try {
				const expiry = new Promise((resolve) => {
					timer = setTimeout(() => resolve(ABANDONED), left);
				});
				const answer = await Promise.race([work(left), expiry]);
				if (answer === ABANDONED) {
					return {
						abandoned: `${step} did not finish within the ${budget} ms ${name} budget and was abandoned`,
					};
				}
				return { value: answer };
			} finally {
				if (timer !== null) {
					clearTimeout(timer);
				}
			}
		},
	};
}

/**
 * The children one plugin instance started.
 *
 * A shutdown must end what is still in flight instead of letting a capture
 * finish into a session that is closing. `defaultExec` tracks every child it
 * spawns; a child that exits on its own leaves the registry, and `killAll` is
 * idempotent so a cleanup that runs twice is safe.
 */
export function createChildRegistry() {
	const children = new Set();
	return {
		track(child) {
			if (!child || typeof child.once !== "function" || typeof child.kill !== "function") {
				return child;
			}
			children.add(child);
			child.once("exit", () => children.delete(child));
			return child;
		},
		killAll() {
			const live = [...children];
			children.clear();
			for (const child of live) {
				try {
					child.kill("SIGTERM");
				} catch {
					// A child that is already gone is gone.
				}
			}
		},
	};
}
