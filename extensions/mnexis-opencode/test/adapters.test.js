/**
 * The host shapes, as the host hands them over.
 *
 * The readers in `src/adapters.js` are deliberately tolerant: a payload this
 * plugin cannot read answers `null` or "" instead of throwing, because a host
 * hook must never fail a turn over a field. Tolerance has one cost — a reader
 * that accepts nothing in particular — so each host event the plugin listens to
 * is pinned here by one recorded payload: the shape RESEARCH.md names, fed to
 * the readers the plugin actually runs on it.
 *
 * The fixtures under `fixtures/` are the documented shapes (RESEARCH §1's hook
 * list and §4's event table), not payloads captured from a live 1.x binary:
 * RESEARCH §7 records that half as unexercised. They are the shape the readers
 * must accept, and the readers keep accepting what they accepted before — no
 * strictness is added here, only coverage.
 */

import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import test from "node:test";
import { fileURLToPath } from "node:url";

import { directoryOfEvent, isHumanMessage, promptTextFromParts, sessionIDOfEvent } from "../index.js";

const FIXTURES = fileURLToPath(new URL("fixtures/", import.meta.url));

function fixture(name) {
	return JSON.parse(readFileSync(new URL(`fixtures/${name}`, import.meta.url), "utf8"));
}

/** The session the fixtures name, in every host shape. */
const SESSION = "ses_01J8Z3K9QX4YV7N2M5P6R8T1WB";

/** One entry per host event the plugin reads: the fixture, and what it must answer. */
const EVENTS = [
	{ file: "v1-session-created.json", session: SESSION, directory: "/home/u/acme" },
	{ file: "v1-session-idle.json", session: SESSION, directory: null },
	{ file: "v1-session-deleted.json", session: SESSION, directory: "/home/u/acme" },
	{ file: "v2-session-created.json", session: SESSION, directory: "/home/u/acme" },
	{ file: "v2-session-deleted.json", session: SESSION, directory: null },
];

test("every recorded host event is read for the session and the directory it names", () => {
	for (const event of EVENTS) {
		const payload = fixture(event.file);
		assert.match(payload.$source, /RESEARCH\.md §\d/, `${event.file} cites where its shape is recorded`);
		assert.equal(sessionIDOfEvent(payload), event.session, event.file);
		assert.equal(directoryOfEvent(payload), event.directory, event.file);
	}
});

test("the recorded V1 chat.message is captured as the person's own words", () => {
	const payload = fixture("v1-chat-message.json");
	assert.match(payload.$source, /RESEARCH\.md §\d/);
	assert.equal(isHumanMessage(payload.output.message), true, "the message is the human's");
	assert.equal(
		promptTextFromParts(payload.output.parts),
		"es importante hacer que la extension sea de calidad",
		"the synthetic part is the host's, and the file part is not text",
	);
	assert.equal(payload.input.sessionID, SESSION);
});

test("every fixture in the directory is pinned by this file, and every pin has its fixture", () => {
	const found = readdirSync(FIXTURES)
		.filter((name) => name.endsWith(".json"))
		.sort();
	const pinned = [...EVENTS.map((event) => event.file), "v1-chat-message.json"].sort();
	assert.deepEqual(found, pinned);
});
