import { test } from "node:test";
import assert from "node:assert/strict";
import {
	DIALOG_TIMEOUT_REASON,
	askConfirm,
	decideAuthorization,
	decideBinding,
	describeFailure,
	allowNamespaceCommand,
} from "../lib/gate.ts";

test("an already authorized namespace is allowed without asking", () => {
	const decision = decideAuthorization({
		namespace: "aegispi",
		writeNamespaces: ["default", "aegispi"],
		hasUI: true,
		mode: "tui",
	});
	assert.deepEqual(decision, { action: "allow" });
});

test("an unauthorized namespace asks for confirmation in an interactive session", () => {
	const decision = decideAuthorization({
		namespace: "aegispi",
		writeNamespaces: ["default"],
		hasUI: true,
		mode: "tui",
	});
	assert.deepEqual(decision, { action: "confirm" });
});

test("a headless session never authorizes: it reports the pending state", () => {
	const decision = decideAuthorization({
		namespace: "aegispi",
		writeNamespaces: ["default"],
		hasUI: false,
		mode: "print",
	});
	assert.equal(decision.action, "pending");
	assert.match(decision.reason, /no interactive UI/i);
});

// R5: the ask keys on `hasUI`, which pi sets in the two dialog-capable modes
// (tui and rpc), and `mode` is not part of the decision any more. The gate
// used to require `mode === "tui"`, so an RPC or IDE client that can draw a
// dialog could never authorize a namespace, and the refusal it got was false.
test("an rpc session with a dialog-capable UI asks, and a headless one does not", () => {
	const rpc = decideAuthorization({
		namespace: "aegispi",
		writeNamespaces: [],
		hasUI: true,
		mode: "rpc",
	});
	assert.deepEqual(rpc, { action: "confirm" }, "a dialog-capable mode asks, whatever its name");

	for (const mode of ["json", "print"]) {
		const headless = decideAuthorization({
			namespace: "aegispi",
			writeNamespaces: [],
			hasUI: false,
			mode,
		});
		assert.equal(headless.action, "pending", `${mode}: no UI, no ask`);
		assert.match(headless.reason, /no interactive UI/i);
	}
});

test("the workspace question follows the same rule: a dialog-capable session asks", () => {
	const asked = decideBinding({ candidate: "aegispi", hasUI: true, mode: "rpc" });
	assert.deepEqual(asked, { action: "confirm" });

	const pending = decideBinding({ candidate: "aegispi", hasUI: false, mode: "print" });
	assert.equal(pending.action, "pending");
	assert.match(pending.reason, /no interactive UI/i);
});

test("the three dialog answers are told apart: confirmed, declined, timed out", async () => {
	const accepting = { confirm: async () => true };
	assert.equal(await askConfirm(accepting, "q", "m", 50), "confirmed");

	const declining = { confirm: async () => false };
	assert.equal(await askConfirm(declining, "q", "m", 50), "declined");

	// pi resolves `false` on a timeout as well, so the dialog that is aborted
	// by the extension's own timer must be read as the timeout it was.
	const silent = {
		confirm: (_title, _message, opts) =>
			new Promise((resolve) => {
				opts.signal.addEventListener("abort", () => resolve(false), { once: true });
			}),
	};
	const started = Date.now();
	assert.equal(await askConfirm(silent, "q", "m", 40), "timed_out");
	assert.ok(Date.now() - started < 2000, "the ask gives up on its own budget");
});

test("the dialog carries its timeout and its abort signal to the host", async () => {
	let seen = null;
	const ui = {
		confirm: async (_title, _message, opts) => {
			seen = opts;
			return true;
		},
	};
	await askConfirm(ui, "q", "m", 4321);
	assert.equal(seen.timeout, 4321, "the dialog auto-dismisses on the budget");
	assert.ok(seen.signal instanceof AbortSignal, "and it carries the signal the timer aborts");
});

test("a timeout is not a decline, in words either", () => {
	assert.match(DIALOG_TIMEOUT_REASON, /not answered in time/i);
	assert.doesNotMatch(DIALOG_TIMEOUT_REASON, /no interactive UI/i);
	assert.doesNotMatch(DIALOG_TIMEOUT_REASON, /declined/i);
});

test("allowNamespaceCommand names the exact command a human must run", () => {
	assert.equal(allowNamespaceCommand("aegispi"), "mnexis config allow-namespace aegispi");
});

test("describeFailure turns exit codes into actionable messages", () => {
	assert.match(describeFailure({ code: 3, stderr: "" }), /not found/i);
	assert.match(
		describeFailure({ code: 4, stderr: "", namespace: "aegispi" }),
		/not authorized for writes/i,
	);
	assert.match(describeFailure({ code: 5, stderr: "" }), /revision|topic/i);
	assert.match(describeFailure({ code: 2, stderr: "bad flag" }), /invalid argument/i);
	assert.match(describeFailure({ code: 2, stderr: "bad flag" }), /bad flag/);
});

test("describeFailure keeps the tail of stderr for diagnosis", () => {
	const message = describeFailure({ code: 1, stderr: "line1\nline2\nline3" });
	assert.match(message, /line3/);
});

// A12: exit 5 is `REVISION_CONFLICT` or `TOPIC_CONFLICT`
// (`src/cli/output.rs:78`), and which of the two a caller can meet follows
// from what it sends. The hint used to say "retry with the current revision"
// for every command, including the ones that carry no revision at all.
test("the exit-5 hint is the truth of the command that failed", () => {
	// A save sends no revision: its conflict is an idempotency key reused for
	// a different payload, or the topic key claimed at the same moment. The
	// hint must not send the model looking for a revision to retry with.
	const saved = describeFailure({ code: 5, stderr: "", command: "save" });
	assert.doesNotMatch(
		saved,
		/retry with the current revision/i,
		`a save has no revision to retry with: ${saved}`,
	);
	assert.match(saved, /idempotency_key/, `the hint names what a save can meet: ${saved}`);

	// A supersede resolves both revisions itself, so its retry is the same
	// supersede, never a revision flag the tool does not take.
	const superseded = describeFailure({ code: 5, stderr: "", command: "supersede" });
	assert.doesNotMatch(
		superseded,
		/retry with the current revision/i,
		`mnexis_supersede takes no revision: ${superseded}`,
	);
	assert.match(superseded, /repeat the same supersede/, `the hint names the retry: ${superseded}`);

	// The command that does take one still says so.
	const updated = describeFailure({ code: 5, stderr: "", command: "update" });
	assert.match(updated, /current revision/i, `an update can name the revision: ${updated}`);

	// And a caller that names no command is told the conflict, not a retry it
	// cannot perform.
	const unknown = describeFailure({ code: 5, stderr: "" });
	assert.match(unknown, /conflict/i, `the default hint names the conflict: ${unknown}`);
});
