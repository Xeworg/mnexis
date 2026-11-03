/**
 * The gate: the two refusals, and the calls that are the model's to make.
 *
 * The plugin's gate is inverted the way this host needs it — the model may not
 * grant a namespace and may not edit the profile by hand, and the refusal names
 * the human's own command. Both plugin models share these rules, so a mistake
 * here is a mistake in two hosts at once.
 */

import assert from "node:assert/strict";
import test from "node:test";

import { GRANT_REFUSAL, PROFILE_REFUSAL, gateViolation } from "../index.js";

test("a shell call that grants a namespace is refused with the command the human must run", () => {
	for (const tool of ["bash", "shell"]) {
		assert.equal(
			gateViolation(tool, { command: "mnexis config allow-namespace acme" }),
			GRANT_REFUSAL,
			tool,
		);
		assert.equal(
			gateViolation(tool, { command: "cd /repo && mnexis  project   bind acme --key k --path ." }),
			GRANT_REFUSAL,
			tool,
		);
	}
	assert.match(GRANT_REFUSAL, /mnexis config allow-namespace/);
	assert.match(GRANT_REFUSAL, /nothing was granted/);
});

test("a shell call that does not grant anything is the model's to make", () => {
	for (const command of [
		"mnexis context 'the port we agreed on'",
		"mnexis config show",
		"mnexis project list",
		"git status",
		"",
	]) {
		assert.equal(gateViolation("bash", { command }), null, command);
	}
	// A tool that carries no command at all is not a grant.
	assert.equal(gateViolation("bash", {}), null);
	assert.equal(gateViolation("bash", undefined), null);
});

test("an edit of the mnexis profile is refused, and nothing else is", () => {
	for (const tool of ["edit", "write", "patch"]) {
		assert.equal(
			gateViolation(tool, { filePath: "/home/u/.mnexis/mnexis.toml" }),
			PROFILE_REFUSAL,
			tool,
		);
		assert.equal(
			gateViolation(tool, { filePath: "~/.mnexis/projects.toml" }),
			PROFILE_REFUSAL,
			tool,
		);
	}
	assert.match(PROFILE_REFUSAL, /managed by the mnexis CLI/);
	assert.equal(gateViolation("write", { filePath: "/repo/notes.toml" }), null);
	assert.equal(gateViolation("write", { filePath: "/repo/src/mnexis.toml.rs" }), null);
	assert.equal(gateViolation("write", { path: "/repo/docs/projects.toml" }), PROFILE_REFUSAL);
	// Reading the profile is not editing it.
	assert.equal(gateViolation("read", { filePath: "/home/u/.mnexis/mnexis.toml" }), null);
	assert.equal(gateViolation("task", { filePath: "/home/u/.mnexis/mnexis.toml" }), null);
});
