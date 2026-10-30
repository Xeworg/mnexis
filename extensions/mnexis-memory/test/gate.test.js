import { test } from "node:test";
import assert from "node:assert/strict";
import { decideAuthorization, describeFailure, allowNamespaceCommand } from "../lib/gate.js";

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

test("a session with UI but a non-tui mode still does not authorize", () => {
	const decision = decideAuthorization({
		namespace: "aegispi",
		writeNamespaces: [],
		hasUI: true,
		mode: "json",
	});
	assert.equal(decision.action, "pending");
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
