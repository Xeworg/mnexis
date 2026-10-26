/**
 * The registered tool surface, and the bench switch that trims it.
 *
 * The prefix experiment (`odd/tasks/prefix-cost.md`, S4) prices the fixed
 * per-call prefix: every registered tool schema is billed on every model
 * call. Arm A registers all of them; arm B registers five. The switch is
 * injected into the factory so no test mutates the process environment.
 */

import { test } from "node:test";
import assert from "node:assert/strict";

import mnexisMemory, { resolveLeanTools } from "../index.js";

/** The surface the shipped extension registers, sorted. */
const ALL_TOOLS = [
	"mnexis_context",
	"mnexis_history",
	"mnexis_list",
	"mnexis_recall",
	"mnexis_save",
	"mnexis_save_prompt",
	"mnexis_search",
	"mnexis_search_prompts",
	"mnexis_session_end",
	"mnexis_session_start",
	"mnexis_show",
	"mnexis_suggest_topic_key",
	"mnexis_supersede",
	"mnexis_workspace",
];

/** The bench arm's surface: the door, the reads that answer, the write. */
const LEAN_TOOLS = [
	"mnexis_context",
	"mnexis_save",
	"mnexis_search",
	"mnexis_show",
	"mnexis_workspace",
];

/** A host that only records the names it is asked to register. */
function fakeHost() {
	const names = [];
	return {
		names,
		pi: {
			registerTool(tool) {
				names.push(tool.name);
			},
			on() {},
		},
	};
}

test("the shipped surface registers every tool", () => {
	const { names, pi } = fakeHost();
	mnexisMemory(pi, { leanTools: false });
	assert.deepEqual([...names].sort(), ALL_TOOLS);
});

test("the lean switch trims the surface to the five bench tools", () => {
	const { names, pi } = fakeHost();
	mnexisMemory(pi, { leanTools: true });
	assert.deepEqual([...names].sort(), LEAN_TOOLS);
});

test("the lean switch is off unless the environment asks for it", () => {
	assert.equal(resolveLeanTools({}, {}), false);
	assert.equal(resolveLeanTools({}, { MNEXIS_BENCH_LEAN_TOOLS: "1" }), true);
	assert.equal(resolveLeanTools({}, { MNEXIS_BENCH_LEAN_TOOLS: "0" }), false);
	assert.equal(resolveLeanTools({}, { MNEXIS_BENCH_LEAN_TOOLS: "" }), false);
});

test("an injected switch is not overridden by the environment", () => {
	assert.equal(resolveLeanTools({ leanTools: false }, { MNEXIS_BENCH_LEAN_TOOLS: "1" }), false);
	assert.equal(resolveLeanTools({ leanTools: true }, {}), true);
});
