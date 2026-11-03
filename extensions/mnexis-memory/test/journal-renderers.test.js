/**
 * The journal the human reads (odd/tasks/ext-ts-rewrite.md, UX1 and UX3).
 *
 * The extension writes four kinds of `pi.appendEntry` journal, and a fifth
 * kind carries the `/mnexis` status card. Without a registered entry renderer
 * those entries are invisible in the chat: the journal only ever reached the
 * session file. What the human reads is one line naming the kind and the fact
 * it holds, and the entry's own data behind it — and the line is built from
 * the entry's `action` and `step`, the variants T4 added, never from an
 * assumed "skipped".
 */

import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import mnexisMemory from "../index.ts";
import { JOURNAL_KINDS, journalLines, journalRenderer } from "../lib/chrome.ts";

/** A host that records what the extension registers, with no engine. */
function fakeHost(t) {
	const dir = mkdtempSync(join(tmpdir(), "mnexis-journal-"));
	t.after(() => rmSync(dir, { recursive: true, force: true }));
	const previous = process.env.MNEXIS_BIN;
	// The session hooks this corpus does not exercise would call the engine;
	// a missing binary is the quietest way to keep the corpus hermetic.
	process.env.MNEXIS_BIN = join(dir, "missing-mnexis");
	t.after(() => {
		if (previous === undefined) {
			delete process.env.MNEXIS_BIN;
		} else {
			process.env.MNEXIS_BIN = previous;
		}
	});

	const entryRenderers = new Map();
	const messageRenderers = new Map();
	const commands = new Map();
	mnexisMemory({
		registerTool() {},
		registerEntryRenderer(customType, renderer) {
			entryRenderers.set(customType, renderer);
		},
		registerMessageRenderer(customType, renderer) {
			messageRenderers.set(customType, renderer);
		},
		registerCommand(name, options) {
			commands.set(name, options);
		},
		on() {},
		appendEntry() {},
	});
	return { entryRenderers, messageRenderers, commands };
}

/** A theme pi could hand a renderer: the one call the drawing makes. */
const THEME = { fg: (color, text) => `<${color}>${text}</${color}>` };

/** The text of the component a renderer returned. */
function drawn(component, width = 100) {
	assert.equal(typeof component?.render, "function", "the renderer returns a component");
	assert.equal(typeof component?.invalidate, "function", "and it can be invalidated");
	return component.render(width);
}

/** One journal entry of each kind, as the extension writes it. */
function entries() {
	return {
		"mnexis-store-recovery": {
			ts: "2026-11-02T10:00:00.000Z",
			kind: "store-recovery",
			session: "01JX",
			action: "supersede",
			namespace: "mnexis",
			older: { uuid: "aaaaaaaa-1", title: "el puerto" },
			newer: { uuid: "bbbbbbbb-2", title: "el puerto del servicio" },
			value: "8081",
			reason: "one fact, one record",
		},
		"mnexis-prompt-capture": {
			ts: "2026-11-02T10:00:00.000Z",
			kind: "prompt-capture",
			session: "01JX",
			action: "skipped",
			bytes: 4096,
			exit_code: 2,
		},
		"mnexis-session-close": {
			ts: "2026-11-02T10:00:00.000Z",
			kind: "session-close",
			session: "01JX",
			action: "noted",
			step: "start",
			already_ended: true,
			ended_at: "2026-11-01T09:00:00.000Z",
			namespace: "mnexis",
		},
		"mnexis-compaction": {
			ts: "2026-11-02T10:00:00.000Z",
			kind: "compaction-capture",
			session: "01JX",
			action: "failed",
			step: "compact",
			reason: "overflow",
			will_retry: false,
			error: "the provider refused the compaction",
		},
		"mnexis-status": {
			ts: "2026-11-02T10:00:00.000Z",
			kind: "status",
			session: "01JX",
			workspace: "git:github.com-xeworg-mnexis",
			namespace: "mnexis",
			bound: true,
			read_namespaces: ["mnexis"],
			write_namespaces: ["mnexis", "default"],
			memories: 7,
			session_state: "started",
			session_ended_at: null,
			prompts_captured: 2,
			counters: { ctx: 1, search: 2, save: 1 },
			footer: "🤖 mnexis · ctx 1 · search 2 · save 1",
			nudge: "open",
		},
	};
}

// --- registration -------------------------------------------------------

test("the four journal kinds and the status card are registered as entry renderers", (t) => {
	const { entryRenderers } = fakeHost(t);

	assert.deepEqual(
		[...entryRenderers.keys()].sort(),
		[...JOURNAL_KINDS].sort(),
		"every kind the extension writes has a renderer",
	);
	assert.deepEqual(
		[...JOURNAL_KINDS].sort(),
		[
			"mnexis-compaction",
			"mnexis-prompt-capture",
			"mnexis-session-close",
			"mnexis-status",
			"mnexis-store-recovery",
		],
		"the four journal kinds, plus the /mnexis card",
	);
	for (const [kind, renderer] of entryRenderers) {
		assert.equal(typeof renderer, "function", `${kind} has a renderer`);
	}
});

test("an older host without entry renderers is left alone", () => {
	// pi before the renderer API: registration is guarded, and the extension
	// still loads and registers its tools.
	assert.doesNotThrow(() => {
		mnexisMemory({
			registerTool() {},
			on() {},
			appendEntry() {},
		});
	});
});

// --- the collapsed line -------------------------------------------------

test("each journal kind renders one line collapsed", (t) => {
	const { entryRenderers } = fakeHost(t);

	for (const [kind, data] of Object.entries(entries())) {
		const lines = drawn(entryRenderers.get(kind)({ customType: kind, data }, { expanded: false }, THEME));
		assert.equal(lines.length, 1, `${kind}: one collapsed line: ${JSON.stringify(lines)}`);
		assert.match(lines[0], /mnexis/, `${kind}: the line names the engine: ${lines[0]}`);
		assert.match(
			lines[0],
			/<dim>/,
			`${kind}: the collapsed line is dim: ${lines[0]}`,
		);
	}
});

test("the collapsed line names the kind and its essential fact", () => {
	const lines = (kind, data) => journalLines(kind, data, { expanded: false });

	const recovery = lines("mnexis-store-recovery", entries()["mnexis-store-recovery"]);
	assert.equal(recovery.length, 1);
	assert.match(recovery[0], /store recovery/, `the kind is named: ${recovery[0]}`);
	assert.match(recovery[0], /8081/, `the shared value is on the line: ${recovery[0]}`);

	const capture = lines("mnexis-prompt-capture", entries()["mnexis-prompt-capture"]);
	assert.match(capture[0], /prompt capture/, `the kind is named: ${capture[0]}`);
	assert.match(capture[0], /4096 B/, `the size is on the line: ${capture[0]}`);

	const close = lines("mnexis-session-close", entries()["mnexis-session-close"]);
	assert.match(close[0], /session close/, `the kind is named: ${close[0]}`);
	assert.match(close[0], /ended/, `the engine's ended state is on the line: ${close[0]}`);

	const compaction = lines("mnexis-compaction", entries()["mnexis-compaction"]);
	assert.match(compaction[0], /compaction/, `the kind is named: ${compaction[0]}`);
	assert.match(compaction[0], /could not compact|failed/, `the failure is on the line: ${compaction[0]}`);

	const status = lines("mnexis-status", entries()["mnexis-status"]);
	assert.match(status[0], /status/, `the card names itself: ${status[0]}`);
	assert.match(status[0], /mnexis/, `the namespace is on the line: ${status[0]}`);
	assert.match(status[0], /7 memories/, `the count is on the line: ${status[0]}`);
});

// --- the variants T4 added ----------------------------------------------

test("session close reads its action and step, never an assumed 'skipped'", () => {
	const line = (data) => journalLines("mnexis-session-close", data, { expanded: false })[0];

	const abandoned = line({ action: "abandoned", step: "end", reason: "the 5 s budget was spent" });
	assert.match(abandoned, /abandoned/, `the action is on the line: ${abandoned}`);
	assert.match(abandoned, /\bend\b/, `the step is on the line: ${abandoned}`);
	assert.doesNotMatch(abandoned, /skipped/, `an abandoned step is not a skip: ${abandoned}`);

	const unregistered = line({ step: "end", exit_code: 3, not_registered: true });
	assert.match(unregistered, /never registered|not registered/i, `exit 3 is named: ${unregistered}`);

	const refusedSave = line({ step: "save", namespace: "mnexis", exit_code: 4 });
	assert.match(refusedSave, /save/, `the step is on the line: ${refusedSave}`);
	assert.match(refusedSave, /4/, `the exit code is on the line: ${refusedSave}`);
	assert.doesNotMatch(refusedSave, /skipped/, `a refused save is not a skip: ${refusedSave}`);

	const started = line({ step: "start", exit_code: 1, error: "the profile is not initialised" });
	assert.match(started, /start/, `the start step is on the line: ${started}`);
	assert.doesNotMatch(started, /skipped/, `a failed start is not a skip: ${started}`);

	const noted = line({ action: "noted", step: "start", already_ended: true });
	assert.match(noted, /ended/, `the noted start says the session is ended: ${noted}`);
	assert.doesNotMatch(noted, /skipped/, `noted is not skipped: ${noted}`);
});

test("compaction tells a refused store from a compaction the host could not run", () => {
	const failed = journalLines(
		"mnexis-compaction",
		{ action: "failed", step: "compact", reason: "overflow", will_retry: true, error: "boom" },
		{ expanded: false },
	)[0];
	const skipped = journalLines(
		"mnexis-compaction",
		{ action: "skipped", step: "save", namespace: "mnexis", exit_code: 5 },
		{ expanded: false },
	)[0];

	assert.match(failed, /could not compact|failed/, `the host failure is named: ${failed}`);
	assert.match(skipped, /not stored|refused/, `the store refusal is named: ${skipped}`);
	assert.notEqual(failed, skipped, "the two variants read differently");
});

test("prompt capture tells an abandoned capture from a refused one", () => {
	const abandoned = journalLines(
		"mnexis-prompt-capture",
		{ action: "abandoned", bytes: 4096, reason: "the 1.5 s budget was spent" },
		{ expanded: false },
	)[0];
	const skipped = journalLines(
		"mnexis-prompt-capture",
		{ action: "skipped", bytes: 4096, exit_code: 2 },
		{ expanded: false },
	)[0];

	assert.match(abandoned, /abandoned/, `an abandoned capture says so: ${abandoned}`);
	assert.match(skipped, /skipped|refused/, `a refused capture says so: ${skipped}`);
	assert.notEqual(abandoned, skipped, "the two variants read differently");
});

test("a store recovery names what it could not finish", () => {
	const failed = journalLines(
		"mnexis-store-recovery",
		{ action: "failed", step: "list", namespace: "mnexis", error: "the store is locked" },
		{ expanded: false },
	)[0];
	assert.match(failed, /store recovery/, `the kind is named: ${failed}`);
	assert.match(failed, /list/, `the step is named: ${failed}`);
	assert.match(failed, /locked/, `the reason is on the line: ${failed}`);
});

// --- the expanded detail -------------------------------------------------

test("expanded, every kind shows the entry's own data", () => {
	for (const [kind, data] of Object.entries(entries())) {
		const lines = journalLines(kind, data, { expanded: true });
		assert.ok(lines.length > 1, `${kind}: the data is expanded: ${JSON.stringify(lines)}`);
		assert.equal(lines[0], journalLines(kind, data, { expanded: false })[0], `${kind}: the line stays on top`);
	}
});

test("the status card expands into the fields it carries", () => {
	const data = entries()["mnexis-status"];
	const text = journalLines("mnexis-status", data, { expanded: true }).join("\n");

	assert.match(text, /git:github\.com-xeworg-mnexis/, `the workspace key: ${text}`);
	assert.match(text, /mnexis/, `the bound namespace: ${text}`);
	assert.match(text, /read/i, `the read allowlist: ${text}`);
	assert.match(text, /write/i, `the write allowlist: ${text}`);
	assert.match(text, /01JX/, `the session id: ${text}`);
	assert.match(text, /started/, `the engine-registered state: ${text}`);
	assert.match(text, /7/, `the memory count: ${text}`);
	assert.match(text, /2/, `the prompts captured: ${text}`);
	assert.match(text, /ctx 1/, `the footer counters: ${text}`);
	assert.match(text, /search 2/, `the footer counters: ${text}`);
	assert.match(text, /save 1/, `the footer counters: ${text}`);
	assert.match(text, /open/, `the nudge state: ${text}`);
});

test("an unbound workspace is drawn as unbound, never as a namespace", () => {
	const data = { workspace: "path:/tmp/x", namespace: null, bound: false };
	const collapsed = journalLines("mnexis-status", data, { expanded: false })[0];
	assert.match(collapsed, /unbound/, `no namespace is drawn as unbound: ${collapsed}`);
});

// --- the renderer pi calls ----------------------------------------------

test("a journal renderer draws a component for an entry the extension wrote", () => {
	const renderer = journalRenderer("mnexis-session-close", THEME);
	const entry = { customType: "mnexis-session-close", data: entries()["mnexis-session-close"] };

	const collapsed = drawn(renderer(entry, { expanded: true }, THEME));
	assert.ok(collapsed.length > 1, `the expanded card draws its data: ${JSON.stringify(collapsed)}`);

	const wide = drawn(renderer({ customType: "mnexis-session-close", data: { action: "abandoned", step: "end", reason: "x".repeat(400) } }, { expanded: false }, THEME), 40);
	assert.ok(
		wide.every((line) => line.replace(/<[^>]+>/g, "").length <= 40),
		`the card wraps inside the width it is given: ${JSON.stringify(wide)}`,
	);
});

test("an entry with no data still renders its kind", () => {
	for (const kind of JOURNAL_KINDS) {
		const lines = journalLines(kind, {}, { expanded: false });
		assert.equal(lines.length, 1, `${kind}: one line without data`);
		assert.match(lines[0], /mnexis/, `${kind}: the line still names the engine: ${lines[0]}`);
	}
});
