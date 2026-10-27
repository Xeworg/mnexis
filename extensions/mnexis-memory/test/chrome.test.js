/**
 * The chrome pi draws around a memory tool: the call line, the panel that
 * names the answer, and the status the footer carries while it runs.
 *
 * These tests pin what the *human* reads in the transcript. What the model
 * reads is out of scope on purpose: the chrome only reads `details`, so the
 * text a tool answered with is exactly what the agent already saw.
 */

import { test } from "node:test";
import assert from "node:assert/strict";

import {
	Panel,
	callLines,
	compactArg,
	displayWidth,
	humanToolName,
	renderers,
	resultLines,
	shortUuid,
	summaryLine,
	typeIcon,
	truncate,
	withStatus,
} from "../lib/chrome.js";

/** The kinds of memory the agent may write. */
const TYPES = [
	"knowledge",
	"decision",
	"preference",
	"procedure",
	"project_state",
	"episode",
	"fact",
];

// --- the name, the icon and the short identity ---

test("a tool is called by its human name", () => {
	assert.equal(humanToolName("mnexis_show"), "show");
	assert.equal(humanToolName("mnexis_suggest_topic_key"), "suggest topic");
});

test("an unknown tool falls back to its own name", () => {
	// The chrome must still draw a tool it does not know: the extension and
	// the drawing are allowed to drift apart without a broken transcript.
	assert.equal(humanToolName("mnexis_remember_all"), "remember all");
	assert.equal(humanToolName("frobnicate_this"), "frobnicate this");
});

test("every kind of memory has its own icon", () => {
	const icons = TYPES.map((type) => typeIcon(type));
	for (const icon of icons) {
		assert.notEqual(icon, "", `a kind of memory is drawn with an icon: ${JSON.stringify(icons)}`);
	}
	assert.equal(new Set(icons).size, TYPES.length, `no two kinds share an icon: ${JSON.stringify(icons)}`);
});

test("an unknown kind of memory is drawn as a fact", () => {
	assert.equal(typeIcon("folklore"), typeIcon("fact"));
});

test("a uuid is said by its first eight characters", () => {
	assert.equal(shortUuid("0123456789abcdef"), "#01234567");
});

test("no uuid is said as nothing", () => {
	assert.equal(shortUuid(""), "");
	assert.equal(shortUuid("   "), "");
	assert.equal(shortUuid(undefined), "");
	assert.equal(shortUuid(null), "");
});

test("a text is collapsed onto one line", () => {
	assert.equal(truncate("  el  puerto\n\tes  8081  ", 80), "el puerto es 8081");
});

test("a text longer than the limit is cut and marked", () => {
	const cut = truncate("x".repeat(60), 10);
	assert.equal(cut, `${"x".repeat(9)}…`, "the cut carries the ellipsis");
	assert.equal(displayWidth(cut), 10, `the cut stays inside the limit: ${cut}`);
	assert.equal(truncate("cabe", 10), "cabe", "a text that fits is left alone");
});

// --- the call line ---

test("the argument of a call is the one worth showing", () => {
	assert.equal(compactArg("mnexis_show", { uuid: "0123456789abcdef" }), "#01234567", "a uuid is short");
	assert.equal(
		compactArg("mnexis_search", { query: "puerto del servicio" }),
		"“puerto del servicio”",
		"a text is quoted",
	);
	assert.equal(compactArg("mnexis_workspace", {}), "", "a tool with no argument shows none");
	assert.equal(compactArg("mnexis_show", {}), "", "a call without its argument shows none");

	const long = compactArg("mnexis_search", { query: "q".repeat(80) });
	assert.ok(long.startsWith("“") && long.endsWith("…”"), `a long text is cut inside its quotes: ${long}`);
});

test("the call line names the tool and its argument", () => {
	const lines = callLines("mnexis_show", { uuid: "0123456789abcdef" });
	assert.equal(lines.length, 1, `a call is one line: ${JSON.stringify(lines)}`);
	assert.match(lines[0], /show/, `the human name is on the line: ${lines[0]}`);
	assert.match(lines[0], /#01234567/, `the argument is on the line: ${lines[0]}`);

	const bare = callLines("mnexis_workspace", {});
	assert.equal(bare.length, 1);
	assert.match(bare[0], /workspace/);
	assert.doesNotMatch(bare[0], /·/, `a call with no argument shows no separator: ${bare[0]}`);
});

// --- the summary line, one per family ---

test("the show summary names the type, the revision and the namespace", () => {
	const line = summaryLine("mnexis_show", {
		details: { memory: { memory_type: "decision", revision: 3, namespace: "mnexis" } },
	});
	assert.equal(line, "↳ ✓ decision · rev 3 · mnexis");
});

test("the list summary counts the memories", () => {
	const line = summaryLine("mnexis_list", { details: { count: 3, memories: [{}, {}, {}] } });
	assert.equal(line, "↳ ✓ 3 memories");
	assert.equal(summaryLine("mnexis_list", { details: { count: 0, memories: [] } }), "↳ ✓ 0 memories");
});

test("the search summary counts the results", () => {
	const line = summaryLine("mnexis_search", { details: { count: 2, hits: [{}, {}] } });
	assert.equal(line, "↳ ✓ 2 results");
});

test("the context summary carries the count and the tokens", () => {
	const line = summaryLine("mnexis_context", { details: { count: 2, tokens_used: 147 } });
	assert.equal(line, "↳ ✓ 2 memories · 147 tokens");
	// An engine that does not declare the budget says nothing about tokens.
	assert.equal(
		summaryLine("mnexis_context", { details: { count: 2, tokens_used: null } }),
		"↳ ✓ 2 memories",
	);
});

test("an error summary carries the reason", () => {
	const line = summaryLine("mnexis_list", {
		isError: true,
		content: [{ type: "text", text: "NOT_FOUND: not found: memory NO-EXISTE" }],
	});
	assert.ok(line.startsWith("↳ ✗"), `the line is marked as a failure: ${line}`);
	assert.match(line, /NO-EXISTE/, `the reason reaches the human: ${line}`);
});

// --- the result panel ---

test("a collapsed answer is one summary line", () => {
	const lines = resultLines("mnexis_list", { details: { count: 3 } });
	assert.equal(lines.length, 1, `the collapsed answer is one line: ${JSON.stringify(lines)}`);
	assert.ok(lines[0].startsWith("↳"), `and it is a summary: ${lines[0]}`);
	assert.match(lines[0], /3 memories/);
});

test("an expanded show draws the memory: its icon, its identity and its text", () => {
	const memory = {
		uuid: "0123456789abcdef",
		memory_type: "decision",
		title: "el puerto del servicio",
		state: "active",
		revision: 4,
		namespace: "mnexis",
		content: "El servicio escucha en el puerto 8081.",
	};
	const lines = resultLines("mnexis_show", { details: { memory } }, { expanded: true });
	const drawn = lines.join("\n");

	assert.ok(drawn.includes(typeIcon("decision")), `the type icon is drawn: ${drawn}`);
	assert.match(drawn, /decision/, `the type is named: ${drawn}`);
	assert.ok(
		lines.some((line) => /^state\s+active$/.test(line)),
		`the state is drawn: ${drawn}`,
	);
	assert.ok(
		lines.some((line) => /^revision\s+4$/.test(line)),
		`the revision is drawn: ${drawn}`,
	);
	assert.ok(
		lines.some((line) => /^namespace\s+mnexis$/.test(line)),
		`the namespace is drawn: ${drawn}`,
	);
	assert.ok(lines.includes(memory.content), `the content is drawn whole: ${drawn}`);
});

test("an expanded list draws one physical line per memory", () => {
	// The engine's own rule: a fact never spans lines. A panel of twenty
	// previews is a list, not a wall of text.
	const memories = [
		{
			uuid: "0123456789abcdef",
			memory_type: "fact",
			title: "el puerto del servicio",
			preview: "el puerto es 8081",
		},
		{ uuid: "fedcba9876543210", memory_type: "decision", title: "usamos postgres", preview: "postgres 16" },
		{ uuid: "aaaabbbbccccdddd", memory_type: "procedure", title: "", preview: "paso uno: correr las pruebas" },
	];
	const lines = resultLines(
		"mnexis_list",
		{ details: { count: memories.length, memories } },
		{ expanded: true },
	);

	assert.equal(
		lines.length,
		memories.length,
		`a memory is one line: ${JSON.stringify(lines)}`,
	);
	assert.equal(
		lines.some((line) => line.includes("\n")),
		false,
		`no line carries a break: ${JSON.stringify(lines)}`,
	);
	assert.ok(
		lines[0].includes(typeIcon("fact")) && lines[0].includes("el puerto del servicio"),
		`a fact shows its icon and its title: ${lines[0]}`,
	);
	assert.ok(
		lines[1].includes(typeIcon("decision")) && lines[1].includes("usamos postgres"),
		`a decision shows its icon and its title: ${lines[1]}`,
	);
	assert.ok(
		lines[2].includes(typeIcon("procedure")) && lines[2].includes("paso uno: correr las pruebas"),
		`a memory with no title shows its preview: ${lines[2]}`,
	);
});

test("an expanded panel says what did not fit", () => {
	const memories = Array.from({ length: 61 }, (_, index) => ({
		uuid: `uuid-${index}`,
		memory_type: "fact",
		title: `memory ${index}`,
	}));
	const lines = resultLines(
		"mnexis_list",
		{ details: { count: memories.length, memories } },
		{ expanded: true },
	);
	assert.equal(lines.length, 61, "the panel keeps every line it was given");

	// The cap counts the lines the terminal shows, not the lines the panel
	// was built from: a wrapped line is two lines on the screen.
	const rendered = new Panel(lines, { maxLines: 60 }).render(80);
	assert.equal(rendered.length, 60, `the panel holds its lines: ${rendered.length}`);
	assert.match(
		rendered[rendered.length - 1],
		/^… \d+ more lines/,
		`the last line says what is left: ${rendered[rendered.length - 1]}`,
	);
});

test("a panel that fits keeps every line", () => {
	const rendered = new Panel(["one", "two"], { maxLines: 60 }).render(80);
	assert.deepEqual(rendered, ["one", "two"], "a panel that fits is untouched");
});

test("a wrapped line counts for the lines it shows", () => {
	const long = "a".repeat(200);
	const rendered = new Panel([long], { maxLines: 4 }).render(40);
	assert.equal(rendered.length, 4, `the panel holds its lines: ${rendered.length}`);
	assert.match(rendered[rendered.length - 1], /^… \d+ more lines/);
});

test("an error answer is one marked line", () => {
	const lines = resultLines("mnexis_search", {
		isError: true,
		content: [{ type: "text", text: "INVALID: the query is empty" }],
	});
	assert.equal(lines.length, 1, `an error is one line: ${JSON.stringify(lines)}`);
	assert.ok(lines[0].startsWith("↳ ✗"), `the line is marked as a failure: ${lines[0]}`);
	assert.match(lines[0], /the query is empty/, `the reason reaches the human: ${lines[0]}`);
});

test("a partial answer is one ellipsis", () => {
	assert.deepEqual(resultLines("mnexis_list", { details: { count: 1 } }, { isPartial: true }), ["…"]);
});

// --- the panel: the width pi gives it ---

test("a panel never draws a line wider than the width it was given", () => {
	const memory = {
		uuid: "0123456789abcdef",
		memory_type: "project_state",
		title: "el puerto del servicio",
		state: "active",
		revision: 4,
		namespace: "mnexis",
		content: "El servicio escucha en el puerto 8081 y el panel lo muestra en el footer. ".repeat(4),
	};
	const lines = resultLines("mnexis_show", { details: { memory } }, { expanded: true });

	for (const width of [24, 40, 72]) {
		const drawn = new Panel(lines).render(width);
		const tooWide = drawn.filter((line) => displayWidth(line) > width);
		assert.deepEqual(
			tooWide,
			[],
			`every line fits ${width} columns: ${JSON.stringify(tooWide)}`,
		);
	}
});

test("a long line wraps instead of being cut", () => {
	const drawn = new Panel(["x".repeat(25)]).render(10);
	assert.ok(drawn.length > 1, `the line wrapped: ${JSON.stringify(drawn)}`);
	assert.equal(drawn.join(""), "x".repeat(25), "no character was lost");
	assert.ok(
		drawn.every((line) => displayWidth(line) <= 10),
		`every wrapped line fits: ${JSON.stringify(drawn)}`,
	);
	assert.deepEqual(new Panel(["aaa bbb ccc"]).render(7), ["aaa bbb", "ccc"], "a line breaks on spaces");
});

test("an empty line still draws one line", () => {
	assert.deepEqual(new Panel([""]).render(80), [""]);
	assert.deepEqual(new Panel("").render(80), [""]);
});

test("a panel can be invalidated and drawn again", () => {
	const panel = new Panel(["a line"]);
	assert.equal(panel.invalidate(), undefined);
	assert.deepEqual(panel.render(80), ["a line"]);
});

// --- the footer status ---

test("the footer carries the status before and after the call", async () => {
	const events = [];
	const answer = { content: [{ type: "text", text: "ok" }], details: {} };
	const params = { uuid: "0123456789abcdef" };
	const signal = { aborted: false };
	const onUpdate = () => {};
	const ctx = { ui: { setStatus: (key, text) => events.push(`${key}:${text}`) } };
	const seen = [];
	const wrapped = withStatus("mnexis_show", async (...args) => {
		events.push("execute");
		seen.push(args);
		return answer;
	});

	const result = await wrapped("t1", params, signal, onUpdate, ctx);

	assert.equal(result, answer, "the answer the tool gave is returned untouched");
	assert.deepEqual(
		seen[0],
		["t1", params, signal, onUpdate, ctx],
		"every argument reaches the tool",
	);
	assert.equal(events[0], "mnexis:🗃️ mnexis · show…", `the status is set before the call: ${events[0]}`);
	assert.equal(events[1], "execute", `and the call runs after it: ${JSON.stringify(events)}`);
	assert.equal(
		events[2],
		"mnexis:🗃️ mnexis · show · ✓",
		`and it says what the tool answered: ${events[2]}`,
	);
});

test("a tool that throws sets the failure status and rethrows", async () => {
	const events = [];
	const ctx = { ui: { setStatus: (key, text) => events.push(`${key}:${text}`) } };
	const boom = new Error("boom");
	const wrapped = withStatus("mnexis_list", async () => {
		throw boom;
	});

	await assert.rejects(wrapped("t2", {}, undefined, undefined, ctx), (error) => error === boom);
	assert.equal(events.length, 2, `one status before, one after: ${JSON.stringify(events)}`);
	assert.equal(events[1], "mnexis:🗃️ mnexis · list · ✗", `the failure is in the footer: ${events[1]}`);
});

test("a tool with no ui still runs", async () => {
	const wrapped = withStatus("mnexis_context", async () => ({ content: [], details: {} }));
	const result = await wrapped("t3", {}, undefined, undefined, {});
	assert.deepEqual(result, { content: [], details: {} });
});

// --- the renderers pi registers ---

test("a tool is registered with its own shell of renderers", () => {
	const chrome = renderers("mnexis_show");
	assert.equal(chrome.renderShell, "self", "the tool draws its own shell");

	const call = chrome.renderCall({ uuid: "0123456789abcdef" });
	assert.equal(typeof call.render, "function", "a call is a component");
	assert.equal(typeof call.invalidate, "function", "and it can be invalidated");
	const callText = call.render(80).join("\n");
	assert.match(callText, /show/, `the call names the tool: ${callText}`);
	assert.match(callText, /#01234567/, `and carries its argument: ${callText}`);

	const result = chrome.renderResult(
		{
			content: [{ type: "text", text: "already inside" }],
			details: {
				memory: {
					uuid: "0123456789abcdef",
					memory_type: "knowledge",
					state: "active",
					revision: 1,
					namespace: "mnexis",
					content: "el motor guarda la memoria",
				},
			},
		},
		{ expanded: true },
	);
	assert.equal(typeof result.render, "function", "a result is a component");
	assert.equal(typeof result.invalidate, "function", "and it can be invalidated");
	const drawn = result.render(80).join("\n");
	assert.ok(drawn.includes(typeIcon("knowledge")), `the panel is the expanded one: ${drawn}`);
	assert.match(drawn, /el motor guarda la memoria/, `the memory is drawn: ${drawn}`);
	assert.equal(result.invalidate(), undefined);
});

test("drawing an answer does not change it", () => {
	// The drawing is a read of `details`: the object the tool returned is
	// the same object after pi drew it, and the model's text is untouched.
	const cases = [
		[
			"mnexis_show",
			{
				content: [{ type: "text", text: "one memory" }],
				details: {
					uuid: "0123456789abcdef",
					memory: {
						uuid: "0123456789abcdef",
						memory_type: "decision",
						title: "el puerto",
						state: "active",
						revision: 2,
						namespace: "mnexis",
						content: "el puerto es 8081",
					},
				},
			},
		],
		[
			"mnexis_list",
			{
				content: [{ type: "text", text: "one memory" }],
				details: {
					count: 1,
					memories: [
						{ uuid: "0123456789abcdef", memory_type: "fact", title: "el puerto", preview: "el puerto es 8081" },
					],
				},
			},
		],
		[
			"mnexis_context",
			{
				content: [{ type: "text", text: "one memory" }],
				details: {
					count: 1,
					tokens_used: 147,
					memories: [
						{ uuid: "0123456789abcdef", memory_type: "fact", title: "el puerto", preview: "el puerto es 8081" },
					],
				},
			},
		],
	];

	for (const [toolName, result] of cases) {
		const before = structuredClone(result);
		const component = renderers(toolName).renderResult(result, { expanded: true });
		component.render(60);
		component.invalidate();
		assert.deepEqual(result, before, `the ${toolName} answer is the one that came back`);
	}
});

test("a name that is not a tool does not reach the prototype", () => {
	assert.equal(humanToolName("toString"), "toString", "an unknown tool is its own name");
	assert.equal(typeIcon("toString"), typeIcon("fact"), "an unknown kind is the fact icon");
	assert.equal(compactArg("toString", {}), "", "an unknown tool has no argument to show");
});

test("the recall panel draws the memories of the window", () => {
	const lines = resultLines(
		"mnexis_recall",
		{
			details: {
				memories: 2,
				recall: {
					previews: [
						{ uuid: "aaaaaaaabbbb", memory_type: "decision", title: "el puerto es 8081" },
						{ uuid: "ccccccccdddd", memory_type: "fact", title: "el puerto es 8081" },
					],
				},
			},
		},
		{ expanded: true },
	);
	assert.equal(lines.length, 2, `the window's memories are drawn: ${JSON.stringify(lines)}`);
	assert.ok(lines[0].startsWith("⚖️ decision"), `the kind leads the line: ${lines[0]}`);
});

test("the workspace panel draws the recent memories", () => {
	const result = {
		details: {
			workspace: { key: "git:github.com-xeworg-mnexis", namespace: "mnexis" },
			status: {
				memory_count: 39,
				recent: [
					{ uuid: "aaaaaaaabbbb", memory_type: "project_state", title: "el board" },
				],
			},
		},
	};
	const summary = summaryLine("mnexis_workspace", result);
	assert.match(summary, /39 memories/, `the summary says how many: ${summary}`);
	const lines = resultLines("mnexis_workspace", result, { expanded: true });
	assert.equal(lines.length, 2, `the workspace and its recent memories: ${JSON.stringify(lines)}`);
	assert.ok(lines[1].startsWith("📍 project_state"), `the kind leads the line: ${lines[1]}`);
});

test("a revision carries when it happened, who did it and why", () => {
	const lines = resultLines(
		"mnexis_history",
		{
			details: {
				count: 1,
				items: [
					{
						revision: 2,
						state: "active",
						changed_at: "2026-10-26T18:00:00Z",
						changed_by: "agent",
						change_reason: "the fact changed",
						content: "the fact says something else now",
					},
				],
			},
		},
		{ expanded: true },
	);
	assert.equal(lines.length, 3, `the revision, its reason and its text: ${JSON.stringify(lines)}`);
	assert.match(lines[0], /2026-10-26T18:00:00Z/, `the revision says when: ${lines[0]}`);
	assert.match(lines[0], /agent/, `the revision says who: ${lines[0]}`);
	assert.match(lines[1], /reason: the fact changed/, `the revision says why: ${lines[1]}`);
	assert.match(lines[2], /the fact says something else now/, `the revision says what: ${lines[2]}`);
});

test("a failed call is marked as a failure, the way pi reports it", () => {
	// pi hands the renderer the content and the details — never the error
	// flag: that lives in the render context, the fourth argument.
	const result = {
		content: [{ type: "text", text: "not found: memory: 191cc344" }],
		details: { uuid: "191cc344" },
	};
	const component = renderers("mnexis_show").renderResult(result, { expanded: true }, undefined, {
		isError: true,
	});
	const lines = component.render(80);
	assert.equal(lines.length, 1, `a failure is one line: ${JSON.stringify(lines)}`);
	assert.ok(lines[0].startsWith("↳ ✗"), `the line is marked as a failure: ${lines[0]}`);
	assert.match(lines[0], /not found/, `the line says what went wrong: ${lines[0]}`);
});

test("a failed call collapsed is a failure too", () => {
	const component = renderers("mnexis_show").renderResult(
		{ content: [{ type: "text", text: "not found: memory: 191cc344" }], details: {} },
		{ expanded: false },
		undefined,
		{ isError: true },
	);
	const lines = component.render(80);
	assert.ok(lines[0].startsWith("↳ ✗"), `the collapsed line is marked: ${lines[0]}`);
});

test("the door shows the content of the records it answers with", () => {
	const lines = resultLines(
		"mnexis_context",
		{
			details: {
				count: 1,
				memories: [
					{
						uuid: "7f5561f7-7493-44c9-b594-3ea1c8c5519e",
						memory_type: "project_state",
						content: "the board is in odd/next.md",
					},
				],
			},
		},
		{ expanded: true },
	);
	assert.equal(lines.length, 2, `the record and its content: ${JSON.stringify(lines)}`);
	assert.ok(lines[0].startsWith("📍 project_state"), `the kind leads the line: ${lines[0]}`);
	assert.match(lines[1], /the board is in odd\/next\.md/, `the content is there: ${lines[1]}`);
});

test("a record that only carries a preview stays one line", () => {
	const lines = resultLines(
		"mnexis_list",
		{
			details: {
				count: 1,
				memories: [
					{
						uuid: "7f5561f7-7493-44c9-b594-3ea1c8c5519e",
						memory_type: "fact",
						preview: "the board is in odd/next.md",
					},
				],
			},
		},
		{ expanded: true },
	);
	assert.equal(lines.length, 1, `a preview is one line: ${JSON.stringify(lines)}`);
});

test("the panel of a record names when it changed and on what basis", () => {
	const lines = resultLines(
		"mnexis_show",
		{
			details: {
				memory: {
					uuid: "191cc344-9560-46cb-b05b-fc1bbe48b8cb",
					memory_type: "fact",
					namespace: "default",
					state: "active",
					revision: 1,
					created_at: "2026-10-20T02:08:58Z",
					updated_at: "2026-10-26T18:00:00Z",
					assertion_basis: "user_stated",
					content: "a fact",
				},
			},
		},
		{ expanded: true },
	);
	assert.ok(
		lines.some((line) => line.startsWith("updated     2026-10-26T18:00:00Z")),
		`the panel says when it changed: ${JSON.stringify(lines)}`,
	);
	assert.ok(
		lines.some((line) => line.startsWith("basis       user_stated")),
		`the panel says on what basis: ${JSON.stringify(lines)}`,
	);
});

test("an empty panel still draws a line", () => {
	const rendered = new Panel([]).render(80);
	assert.equal(rendered.length, 1, `the panel is never blank: ${JSON.stringify(rendered)}`);
});

test("a joined emoji is one glyph, and a wide letter is two columns", () => {
	assert.equal(displayWidth("👨‍👩‍👧"), 2, "a joined sequence is one glyph");
	assert.equal(displayWidth("漢"), 2, "a wide letter takes two columns");
	assert.equal(displayWidth("🧠"), 2, "an emoji takes two columns");
	assert.equal(displayWidth("a"), 1, "a letter takes one column");
});

test("a count speaks in the singular when it is one", () => {
	assert.equal(summaryLine("mnexis_list", { details: { count: 1 } }), "↳ ✓ 1 memory");
	assert.equal(summaryLine("mnexis_list", { details: { count: 2 } }), "↳ ✓ 2 memories");
	assert.equal(summaryLine("mnexis_search", { details: { count: 1 } }), "↳ ✓ 1 result");
	assert.equal(
		summaryLine("mnexis_context", { details: { count: 1, tokens_used: 1156 } }),
		"↳ ✓ 1 memory · 1156 tokens",
	);
});
