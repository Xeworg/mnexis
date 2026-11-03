/**
 * The words the plugin renders, without a host and without the binary.
 *
 * Three things reach a model as text and are not the host's to compose: the
 * pointer to the door, the block a compaction keeps, and the state a session
 * closes with. Each is a pure function of the engine's own answer, so each is
 * pinned here. `promptTextFromParts`, `isHumanMessage` and `sessionIDOfEvent`
 * read the host's shapes and are pinned beside them: a shape this plugin
 * misreads is a rule it never applies.
 */

import assert from "node:assert/strict";
import test from "node:test";

import {
	STATE_CHARS,
	SUMMARY_CHARS,
	isHumanMessage,
	pointerText,
	promptTextFromParts,
	sessionCloseContent,
	sessionIDOfEvent,
	stateBlockText,
} from "../index.js";

const STATUS = {
	workspace: { key: "git:example.com-acme", namespace: "acme", candidate_namespace: null },
	memory_count: 42,
};

const PREVIOUS = {
	session: {
		id: "sess-previous",
		started_at: "2026-10-30T22:04:11Z",
		summary: "Wired the budget into the door.\nThe second line is not carried.",
	},
};

test("the text of a prompt is its own words, never the host's", () => {
	assert.equal(
		promptTextFromParts([
			{ type: "text", text: "haz la extension de opencode de calidad" },
			{ type: "file", mime: "image/png", url: "data:image/png;base64,AA" },
		]),
		"haz la extension de opencode de calidad",
	);
	assert.equal(
		promptTextFromParts([
			{ type: "text", text: "what the human said" },
			{ type: "text", text: "auto-continue", synthetic: true },
		]),
		"what the human said",
	);
	assert.equal(promptTextFromParts([{ type: "text", text: "only the host", synthetic: true }]), "");
	assert.equal(promptTextFromParts([]), "");
	assert.equal(promptTextFromParts(undefined), "");
	assert.equal(promptTextFromParts([{ type: "text" }, { type: "text", text: "   " }]), "");
});

test("a message the host sent is not a message the human typed", () => {
	assert.equal(isHumanMessage({ role: "user" }), true);
	assert.equal(isHumanMessage({}), true);
	assert.equal(isHumanMessage({ role: "assistant" }), false);
	assert.equal(isHumanMessage(null), true);
});

test("the session id an event names is read in both hosts' shapes", () => {
	// opencode 1.x
	assert.equal(
		sessionIDOfEvent({ type: "session.created", properties: { info: { id: "oc-1", directory: "/repo" } } }),
		"oc-1",
	);
	// opencode 2.x
	assert.equal(sessionIDOfEvent({ type: "session.created", data: { sessionID: "oc-2" } }), "oc-2");
	assert.equal(sessionIDOfEvent({ type: "session.idle", properties: { sessionID: "oc-3" } }), "oc-3");
	assert.equal(sessionIDOfEvent({ type: "session.created" }), null);
	assert.equal(sessionIDOfEvent(null), null);
});

test("the compaction block is the engine's live state, bounded, and says what did not fit", () => {
	const data = {
		count: 2,
		items: [
			{
				uuid: "8b4f9c2e-1111-4a1a-9f1f-1e2d3c4b5a69",
				title: "The door answers previews",
				memory_type: "decision",
				revision: 3,
				content: "memory_context answers a 160-character preview per item; the whole record is memory_get.",
			},
			{ uuid: "u2", title: "", memory_type: "project_state", state: "active", content: "" },
		],
	};
	const block = stateBlockText(data);
	assert.match(block, /^mnexis — the live state of this project/);
	assert.match(block, /- \[decision\] The door answers previews \(8b4f9c2e-1111-4a1a-9f1f-1e2d3c4b5a69, rev 3\)/);
	assert.match(block, /160-character preview per item/);
	// A record with no content carries nothing: an empty line in the block
	// would be a fact the model reads as "there is nothing more".
	assert.doesNotMatch(block, /\(u2/);

	assert.equal(stateBlockText({ items: [] }), "");
	assert.equal(stateBlockText({ items: [{ uuid: "u", content: "   " }] }), "");
	assert.equal(stateBlockText(null), "");
});

test("the compaction block stops at its budget and counts what it left out", () => {
	const items = [];
	for (let i = 0; i < 6; i += 1) {
		items.push({
			uuid: `u${i}`,
			title: `record ${i}`,
			memory_type: "decision",
			content: "x".repeat(Math.floor(STATE_CHARS / 4)),
		});
	}
	const block = stateBlockText({ items });
	const carried = (block.match(/^- \[/gm) ?? []).length;
	assert.ok(carried >= 1 && carried < 6, `carried ${carried}`);
	assert.match(block, new RegExp(`… ${6 - carried} more live record`));
});

test("the pointer names the namespace, the door and the previous session", () => {
	const text = pointerText({ status: STATUS, previous: PREVIOUS });
	assert.match(text, /namespace `acme` \(42 memories\)/);
	assert.match(text, /mnexis_memory_context/);
	// The tool has a second name under opencode 2's Code Mode, and a dead
	// reference would be worse than a longer line.
	assert.match(text, /tools\.mnexis\.memory_context/);
	assert.match(text, /Previous session \(2026-10-30\): Wired the budget into the door\./);
	assert.doesNotMatch(text, /second line/, "the pointer carries one line, not the whole summary");
	assert.match(text, /never copy the conversation into `memory_save_prompt`/);
});

test("the pointer says nothing when there is no namespace to name", () => {
	assert.equal(pointerText({ status: { workspace: { namespace: null } } }), "");
	assert.equal(pointerText({ status: {} }), "");
	assert.equal(pointerText({}), "");
	assert.equal(pointerText(), "");
	// A count the engine did not answer is not "0 memories".
	assert.match(pointerText({ status: { workspace: { namespace: "acme" } } }), /namespace `acme`\./);
});

test("the pointer bounds the previous summary it carries", () => {
	const long = "y".repeat(SUMMARY_CHARS * 2);
	const text = pointerText({ status: STATUS, previous: { session: { started_at: "", summary: long } } });
	const line = text.split("\n").find((l) => l.startsWith("Previous session"));
	assert.ok(line.includes("date unknown"));
	assert.ok(line.length <= SUMMARY_CHARS + 40, `line of ${line.length}`);
	assert.ok(line.endsWith("…"));
	// No summary recorded is not an error and draws nothing extra.
	const without = pointerText({ status: STATUS, previous: { session: { summary: null } } });
	assert.equal(without.split("\n").some((l) => l.startsWith("Previous session")), false);
});

test("the estado al cierre states only what the engine answered", () => {
	const content = sessionCloseContent({
		id: "s1",
		namespace: "acme",
		directory: "/repo",
		started_at: "2026-11-03T10:00:00Z",
		ended_at: "2026-11-03T10:30:00Z",
		branch: "feat/opencode-parity",
		branch_sha: "abc123",
		branch_remote: "git:example.com-acme",
	});

	assert.equal(
		content,
		[
			"Session s1 ended at 2026-11-03T10:30:00Z.",
			"It started at 2026-11-03T10:00:00Z.",
			"Directory: /repo.",
			"Branch: feat/opencode-parity at abc123 (git:example.com-acme).",
		].join("\n"),
	);
	// A fact the engine did not answer is omitted, never written as `undefined`.
	assert.equal(sessionCloseContent({ id: "s2" }), "Session s2 ended.");
	assert.equal(sessionCloseContent(null), "Session unknown ended.");
});
