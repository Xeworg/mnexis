/**
 * The door's answer, as the model reads it.
 *
 * Three renderings of the same engine answer, and only one of them is
 * shipped:
 *
 * - `""` (the shipped one): the budgeted items as fat JSON, one per line,
 *   plus the declaration line. Byte-identical to what the door answered
 *   before this module existed.
 * - `block`: the same items, plus the session-state block as JSON. The
 *   block is what `MemoryService::context` attaches to the answer
 *   (`response.session_state`) and what the tool's own description promises
 *   the model.
 * - `shape`: the same items **and** the same block, rendered in the shape
 *   the chrome draws for the human — one physical line per memory, the kind
 *   of the fact next to its name, the identity of the record before its
 *   text, the text indented under it.
 *
 * The last two are the bench's arms (odd/tasks/chrome-shape.md, S3): the
 * switch is read once from the environment and is absent in the shipped
 * product, where the door answers exactly as it always did.
 */

import { shapeRecords } from "./chrome.js";

/** The renderings the bench may ask for. The empty one is the shipped one. */
export const DOOR_MODES = ["block", "shape"];

/**
 * The rendering the door answers in.
 *
 * The variable is read once, from the environment, and anything that is not
 * one of the two bench modes is the shipped answer — a typo in a container
 * must never change what the model reads.
 */
export function resolveDoorMode(value) {
	const mode = String(value ?? "")
		.trim()
		.toLowerCase();
	return DOOR_MODES.includes(mode) ? mode : "";
}

/**
 * The session-state block, as the engine sent it.
 *
 * `null` when the envelope carries no block, or when the block carries
 * nothing: an empty block adds a section that says nothing, and a section
 * that says nothing is a cost.
 */
export function readBlock(payload) {
	if (!payload || typeof payload !== "object") return null;
	const raw = payload.session_state;
	if (!raw || typeof raw !== "object") return null;
	const records = Array.isArray(raw.records) ? raw.records : [];
	const session = raw.session && typeof raw.session === "object" ? raw.session : null;
	if (records.length === 0 && !session) return null;
	return { records, session };
}

/**
 * The text the model reads from the door.
 *
 * The same content in every rendering: the items the engine selected, the
 * declaration it wrote, and the block it attached. Only the rendering
 * changes, so two arms that differ by rendering carry the same facts.
 */
export function doorAnswer({ items = [], block = null, declaration = "", mode = "", task = "" } = {}) {
	const empty = `No memories answered the task '${task ?? ""}'.`;
	if (mode === "shape") {
		const sections = [];
		sections.push(items.length > 0 ? shapeRecords(items).join("\n") : empty);
		if (declaration) sections.push(declaration);
		if (block) {
			if (block.records.length > 0) {
				sections.push(`Session state:\n${shapeRecords(block.records).join("\n")}`);
			}
			if (block.session) {
				const summary = block.session.summary ?? "(no summary)";
				sections.push(`Previous session:\n${indent(summary)}`);
			}
		}
		return sections.join("\n\n");
	}
	const body =
		items.length > 0
			? items.map((item, index) => `${index + 1}. ${JSON.stringify(item)}`).join("\n")
			: empty;
	const sections = [body, declaration];
	if (mode === "block" && block) {
		sections.push(`Session state: ${JSON.stringify(block)}`);
	}
	return sections.join("\n");
}

/** The text of a record, one line per line, indented under its heading. */
function indent(text, prefix = "   ") {
	return String(text ?? "")
		.split("\n")
		.map((line) => `${prefix}${line}`)
		.join("\n");
}
