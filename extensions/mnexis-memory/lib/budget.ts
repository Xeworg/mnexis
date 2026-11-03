/**
 * The wall-clock budgets of the work pi awaits without a timeout.
 *
 * Two hooks run inside an await the human pays for: `session_shutdown`, which
 * pi awaits with no timeout on /new, /resume, /fork and quit, and the `input`
 * hook, which runs on every message the human sends. Per-call timeouts bound
 * each engine call, but a step can be several calls (an end, its busy retries,
 * a save), and the sum is what the human waits. So the work gets one deadline
 * of its own: `step` runs one piece with whatever is left and abandons what
 * does not fit instead of waiting for it (ext-ts-rewrite R1, R7).
 */

/**
 * How long every step of a session close may take together.
 *
 * The close is two engine writes what the next session needs: the end and the
 * estado al cierre. Five seconds is already pathological for two local writes;
 * past it the state is worth less than the human's quit. An abandoned close is
 * journaled and re-runnable: the end is idempotent by construction and the
 * estado al cierre is one record under one title, so the next session revises
 * both.
 */
export const SHUTDOWN_BUDGET_MS = 5_000;

/**
 * How long the capture of one message may take.
 *
 * The capture is one indexed insert, and it happens on every message: the turn
 * is the human's, and a store that cannot take the message in a second and a
 * half is a store the turn should not wait for (R7).
 */
export const PROMPT_CAPTURE_BUDGET_MS = 1_500;

/**
 * A budget as the journal says it: `300 ms`, `1.5 s`, `5 s`.
 */
export function budgetLabel(ms) {
	const value = Number(ms);
	if (!Number.isFinite(value) || value <= 0) {
		return "0 ms";
	}
	if (value < 1000) {
		return `${Math.round(value)} ms`;
	}
	return value % 1000 === 0 ? `${value / 1000} s` : `${Math.round(value / 100) / 10} s`;
}

/**
 * The budget an environment override names, or the default.
 *
 * The same shape the CLI's own budgets take: a positive integer wins, anything
 * else is the default. It exists so the corpus pays a fifth of a second for a
 * conclusion that would otherwise cost five, and so a host with a slow store
 * can raise the ceiling without a new build.
 */
export function budgetOf(override, fallback) {
	const value = Number.parseInt(String(override ?? ""), 10);
	return Number.isFinite(value) && value > 0 ? value : fallback;
}

/** The token a step gets when it ran out of time: never a value it could return. */
const ABANDONED = Symbol("abandoned");

/**
 * A deadline, in milliseconds from now.
 *
 * @param {number} budgetMs how long the whole piece of work may take
 * @param {string} label what the work is, for the sentence a journal carries
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

		/**
		 * Run one step with what is left of the budget.
		 *
		 * `work` receives the milliseconds it has, so an engine call can be given
		 * exactly that and its child dies at the same moment the step does. The
		 * answer is `{ value }` or `{ abandoned: reason }` — never a rejection
		 * the caller has to tell apart from an engine failure.
		 *
		 * @param {string} step what this piece is, for the journal
		 * @param {(budgetMs: number) => Promise<any>} work the call to bound
		 */
		async step(step, work) {
			const left = this.remaining();
			if (left <= 0) {
				return {
					abandoned:
						`${step} was abandoned: the ${budgetLabel(budget)} ${name} budget was ` +
						"already spent",
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
						abandoned:
							`${step} did not finish within the ${budgetLabel(budget)} ${name} ` +
							"budget and was abandoned",
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
