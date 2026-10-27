/**
 * The door's model-facing answer: three renderings of the same engine
 * answer, and only one of them is shipped.
 *
 * The bench's arms (odd/tasks/chrome-shape.md, S3) compare renderings of
 * the same content, so these tests pin two things: that the shipped
 * rendering does not move, and that the other two carry the same facts.
 */

import { test } from "node:test";
import assert from "node:assert/strict";

import { doorAnswer, readBlock, resolveDoorMode } from "../lib/door.js";

const ITEM = {
	uuid: "902a147e-bb07-4d5e-816e-4d5bdee64d08",
	title: "Where the project is",
	memory_type: "project_state",
	content: "The door re-measure closed with 2 of 5 criteria.",
	keywords: ["estado"],
	revision: 1,
	namespace: "payments",
	state: "active",
	topic_key: "where-the-project-is",
	assertion_basis: "user_stated",
	created_at: "2026-10-27T03:24:56Z",
};

const BLOCK = {
	records: [
		{
			uuid: "902a147e-bb07-4d5e-816e-4d5bdee64d08",
			memory_type: "project_state",
			title: "Where the project is",
			content: "The door re-measure closed with 2 of 5 criteria.",
			created_at: "2026-10-27T03:24:56Z",
		},
	],
	session: { id: "s1", namespace: "payments", ended_at: "2026-10-27T02:00:00Z", summary: "Closed the batch." },
};

const DECLARATION = "Namespace searched: payments. Tokens: 178. Scope: restricted.";

test("the shipped rendering is the items as JSON, with the declaration", () => {
	const text = doorAnswer({ items: [ITEM], block: null, declaration: DECLARATION, mode: "" });
	assert.equal(
		text,
		`1. ${JSON.stringify(ITEM)}\n${DECLARATION}`,
		`the shipped answer does not move: ${text}`,
	);
	assert.doesNotMatch(text, /Session state/, "and it carries no block");
});

test("the block rendering adds the session state as JSON, and nothing else moves", () => {
	const text = doorAnswer({ items: [ITEM], block: BLOCK, declaration: DECLARATION, mode: "block" });
	assert.ok(text.startsWith(`1. ${JSON.stringify(ITEM)}`), `the items come first: ${text}`);
	assert.ok(text.includes(DECLARATION), `the declaration stays: ${text}`);
	assert.match(text, /Session state: \{/, `the block is there: ${text}`);
	assert.ok(text.includes("Closed the batch."), `the block carries the session: ${text}`);
});

test("the shape rendering names the kind, the identity and the text", () => {
	const text = doorAnswer({ items: [ITEM], block: BLOCK, declaration: DECLARATION, mode: "shape" });
	assert.match(text, /📍 project_state · Where the project is · #902a147e/, `the kind, the name and the identity: ${text}`);
	assert.match(text, /The door re-measure closed with 2 of 5 criteria\./, `and the text: ${text}`);
	assert.ok(!text.includes('"'), `the shape is not JSON: ${text}`);
	assert.ok(text.includes(DECLARATION), `the declaration stays: ${text}`);
	assert.match(text, /Session state:/, `the block is there: ${text}`);
	assert.match(text, /Previous session:/, `and the previous session: ${text}`);
});

test("both renderings carry the same facts", () => {
	const json = doorAnswer({ items: [ITEM], block: BLOCK, declaration: DECLARATION, mode: "block" });
	const shape = doorAnswer({ items: [ITEM], block: BLOCK, declaration: DECLARATION, mode: "shape" });
	for (const fact of ["902a147e", "Where the project is", "The door re-measure closed with 2 of 5 criteria."]) {
		assert.ok(json.includes(fact), `the JSON carries ${fact}: ${json}`);
		assert.ok(shape.includes(fact), `the shape carries ${fact}: ${shape}`);
	}
});

test("an empty block adds nothing at all", () => {
	const shipped = doorAnswer({ items: [ITEM], block: null, declaration: DECLARATION, mode: "" });
	const asBlock = doorAnswer({ items: [ITEM], block: null, declaration: DECLARATION, mode: "block" });
	assert.equal(asBlock, shipped, `the block rendering without a block does not move: ${asBlock}`);
	const asShape = doorAnswer({ items: [ITEM], block: null, declaration: DECLARATION, mode: "shape" });
	assert.doesNotMatch(asShape, /Session state/, `the shape without a block adds no section: ${asShape}`);
});

test("an answer with no items says so", () => {
	const text = doorAnswer({ items: [], block: null, declaration: DECLARATION, mode: "shape", task: "el puerto" });
	assert.match(text, /No memories answered the task 'el puerto'\./, `the empty answer says so: ${text}`);
});

test("the switch only accepts the two bench modes", () => {
	assert.equal(resolveDoorMode("shape"), "shape");
	assert.equal(resolveDoorMode("BLOCK"), "block");
	assert.equal(resolveDoorMode(" block "), "block");
	assert.equal(resolveDoorMode(""), "");
	assert.equal(resolveDoorMode(undefined), "");
	assert.equal(resolveDoorMode("nonsense"), "", "a typo must never change what the model reads");
});

test("the block is read from the envelope, and an empty one is nothing", () => {
	assert.equal(readBlock({ items: [] }), null);
	assert.equal(readBlock({ session_state: { records: [], session: null } }), null);
	const block = readBlock({ session_state: BLOCK });
	assert.equal(block.records.length, 1);
	assert.equal(block.session.summary, "Closed the batch.");
});
