/**
 * The budgets and the children: the primitives every call is built on.
 *
 * A host hook must never hold a turn waiting for the engine, so one piece of
 * work runs inside a deadline that abandons what does not fit; and a child still
 * in flight at unload is the plugin's to end. Neither of them needs the binary.
 */

import assert from "node:assert/strict";
import test from "node:test";

import { createChildRegistry, createDeadline } from "../index.js";

test("a step that runs out of budget is abandoned, never awaited forever", async () => {
	const deadline = createDeadline(20, "prompt capture");

	const answer = await deadline.step("capture", () => new Promise(() => {}));

	assert.match(answer.abandoned, /capture did not finish within the .* prompt capture budget/);
});

test("a step that fits keeps its answer, and a spent budget abandons without work", async () => {
	const deadline = createDeadline(500, "pointer");
	const answer = await deadline.step("status", async () => "ok");
	assert.equal(answer.value, "ok");

	const spent = createDeadline(0, "pointer");
	let ran = false;
	const skipped = await spent.step("status", async () => {
		ran = true;
		return "never";
	});
	assert.match(skipped.abandoned, /already spent/);
	assert.equal(ran, false, "work with no budget left is never started");
});

test("the child registry kills what it tracks, once, and forgets what exited", () => {
	const registry = createChildRegistry();
	const killed = [];
	const exited = [];
	const first = {
		kill: (signal) => killed.push(signal),
		once: (name, callback) => exited.push({ name, callback }),
	};
	registry.track(first);
	registry.track({ kill: () => killed.push("SIGTERM"), once: () => {} });

	// A child that exits on its own is no longer the registry's to kill.
	exited.find((entry) => entry.name === "exit").callback();

	registry.killAll();
	assert.deepEqual(killed, ["SIGTERM"]);
	registry.killAll();
	assert.deepEqual(killed, ["SIGTERM"], "the registry is emptied by the first kill");
});
