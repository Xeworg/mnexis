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
