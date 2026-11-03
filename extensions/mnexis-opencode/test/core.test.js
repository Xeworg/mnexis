/**
 * The decisions, without a host and without the binary.
 *
 * Every one of these is a rule the plugin applies on both plugin models, so
 * a mistake here is a mistake in two hosts at once. The drift pins at the end
 * read the repository's own files: the plugin is one flat file, so its curated
 * copy of pi's vocabulary and the engine's tool ids has to be checked against
 * the files it was copied from.
 */

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import mnexisMemory from "../../mnexis-memory/index.ts";

import {
	CURATED_DESCRIPTIONS,
	GRANT_REFUSAL,
	PROFILE_REFUSAL,
	STATE_CHARS,
	SUMMARY_CHARS,
	TYPE_GLOSSES,
	gateViolation,
	isHumanMessage,
	parseJson,
	pointerText,
	promptTextFromParts,
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

test("the CLI's JSON is read even when something else wrote around it", () => {
	assert.deepEqual(parseJson('{"a":1}'), { a: 1 });
	assert.deepEqual(parseJson('\nwarning: x\n{"a":1}\n'), { a: 1 });
	assert.equal(parseJson(""), null);
	assert.equal(parseJson("not json"), null);
});

// --- the drift pins (odd/tasks/opencode-polish.md, S5) --------------------
//
// The curated definitions and the seven glosses are an inline copy in the one
// flat file the plugin must stay, because the plugin cannot import from the
// engine or from pi. A copy that may drift is a copy nobody can trust, so each
// pin reads the file the copy came from, from the repository root.

const REPO_ROOT = fileURLToPath(new URL("../../..", import.meta.url));

/** A repository file, read from disk — the copy is pinned to the file. */
function repoFile(relative) {
	return readFileSync(join(REPO_ROOT, relative), "utf8");
}

/**
 * The seven type glosses a text states, as `kind — gloss` lines.
 *
 * The plugin states them as string literals in one flat file; pi states them
 * as the lines of `mnexis_save`'s description (they moved there in T3, AB5).
 * Both write one kind per line, so the pin is textual and carrier-agnostic:
 * the same gloss in another carrier is the same line here, and a reworded one
 * is a different line even when both sides would still be exported and
 * imported.
 */
function glossLines(text) {
	const lines = [];
	for (const line of String(text ?? "").split("\n")) {
		const match = line.match(
			/^\s*"?(knowledge|decision|preference|procedure|project_state|episode|fact) — (.+?)"?,?$/,
		);
		if (match) {
			lines.push(`${match[1]} — ${match[2]}`);
		}
	}
	return lines;
}

test("the seven type glosses in the plugin are exactly the ones pi's save description carries", () => {
	const plugin = glossLines(repoFile("extensions/mnexis-opencode/index.js"));
	// The carrier changed in T3 (AB5): the taxonomy moved out of the settle
	// nudge and into the save description, where the model reads it at the
	// moment of the write. The parity intent does not: one vocabulary, two
	// hosts, and neither may reword a kind on its own.
	const tools = new Map();
	mnexisMemory({
		registerTool: (tool) => tools.set(tool.name, tool),
		on() {},
		appendEntry() {},
	});
	const save = glossLines(tools.get("mnexis_save").description);

	assert.equal(
		save.length,
		7,
		`the save description states the seven kinds once: ${save.join(" | ")}`,
	);
	assert.deepEqual(
		plugin,
		save,
		"the plugin's glosses are pi's, verbatim: one definition of the vocabulary, two consumers",
	);
	assert.deepEqual(plugin, TYPE_GLOSSES, "and the exported list is the one the save description carries");
});

test("the curated map carries exactly one non-empty description per MCP tool id", () => {
	// The engine's tool inventory is the id list (src/mcp/server.rs). The host
	// namespaces an MCP tool `<server>_<tool>`, so `memory_save` is listed as
	// `mnexis_memory_save`.
	const ids = [...repoFile("src/mcp/server.rs").matchAll(/\[tool\(\s*name = "([a-z_]+)"/g)].map(
		(match) => `mnexis_${match[1]}`,
	);
	assert.equal(ids.length, 17, `the engine publishes its tool inventory: ${ids}`);

	assert.deepEqual(
		Object.keys(CURATED_DESCRIPTIONS).sort(),
		[...ids].sort(),
		"one curated entry per engine tool, and no id the engine does not publish",
	);
	for (const [id, text] of Object.entries(CURATED_DESCRIPTIONS)) {
		assert.equal(typeof text, "string", id);
		assert.notEqual(text.trim(), "", `the description of ${id} is not empty`);
	}
});

/** pi's declared tools, prose only: the repo's own measurement.
 *
 * The loadout T3 decides is what the model is declared on every request: the
 * `direct`, active tools. A `deferred` or `hidden` tool's description is
 * registered but never declared (odd/tasks/ext-ts-rewrite.md, rules 3 and 7),
 * so it is not part of this bill. */
function piToolsProse() {
	const tools = new Map();
	mnexisMemory({
		registerTool: (tool) => tools.set(tool.name, tool),
		on() {},
		appendEntry() {},
	});
	const parts = [];
	for (const tool of tools.values()) {
		if ((tool.exposure ?? "direct") !== "direct" || tool.defaultActive === false) {
			continue;
		}
		parts.push(tool.description);
		for (const property of Object.values(tool.parameters?.properties ?? {})) {
			parts.push(property?.description ?? "");
		}
	}
	return Buffer.byteLength(parts.join(""), "utf8");
}

test("pi's declared tools block still fits the 5500 B pin the curated map was not allowed to grow", () => {
	// The vocabulary has two consumers (S1-L2): pi's save description and this
	// plugin's save description. T3 moved the glosses into pi's save (AB5),
	// so this pin now measures the block they ride in — the tools pi declares
	// on every request, with the same 5 500 B budget.
	const bytes = piToolsProse();
	assert.ok(bytes <= 5500, `pi's declared tools block is ${bytes} B (budget 5500 B)`);
});
