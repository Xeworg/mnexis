/**
 * The chrome pi draws around the mnexis tools.
 *
 * What this is: the drawing the *human* reads in the transcript — the call
 * line, and the panel that names a memory when the answer comes back.
 *
 * What this is not: a change to what the model reads. The text a tool
 * returns is exactly what the CLI answered; this module only reads
 * `details` to draw what the human reads. That is why it lives here and not
 * in the CLI: the CLI's stdout is the machine channel of every agent (this
 * extension, opencode, codex, claude code), and a drawing there is paid by
 * every agent, in tokens, for every memory it reads. Here it is free: pi
 * draws the call and the result, and the model reads the plain text.
 *
 * The drawing follows the engine's own rules: one physical line per memory
 * in a list, the type of the fact next to its name, and the identity of a
 * record before its text.
 */

/** The human name of every tool, the way the footer and the call line say it. */
export const TOOL_LABELS = {
	mnexis_save: "save",
	mnexis_search: "search",
	mnexis_suggest_topic_key: "suggest topic",
	mnexis_list: "list",
	mnexis_show: "show",
	mnexis_history: "history",
	mnexis_session_start: "start session",
	mnexis_session_end: "end session",
	mnexis_save_prompt: "save prompt",
	mnexis_search_prompts: "search prompts",
	mnexis_recall: "recall",
	mnexis_workspace: "workspace",
	mnexis_context: "context",
	mnexis_supersede: "supersede",
};

/** The one argument worth showing for each tool. */
export const ARG_KEYS = {
	mnexis_save: ["title"],
	mnexis_search: ["query"],
	mnexis_suggest_topic_key: ["title"],
	mnexis_list: ["namespace"],
	mnexis_show: ["uuid"],
	mnexis_history: ["uuid"],
	mnexis_session_start: ["session_id"],
	mnexis_session_end: ["session_id"],
	mnexis_save_prompt: ["session_id"],
	mnexis_search_prompts: ["query"],
	mnexis_recall: ["session"],
	mnexis_workspace: [],
	mnexis_context: ["task"],
	mnexis_supersede: ["older"],
};

/**
 * The icon of every kind of memory — the closed set the agent writes.
 *
 * Every one of them is a bright, saturated emoji: the dark ones (the scales,
 * the compass, the filing cabinet) were unreadable on a dark terminal, and a
 * mark the human cannot see is not a mark.
 */
export const TYPE_ICONS = {
	knowledge: "📘",
	decision: "🎯",
	preference: "⭐",
	procedure: "📋",
	project_state: "📍",
	episode: "🎉",
	fact: "🧠",
};

/** The engine's own mark, so a session with two memory engines stays readable. */
const ENGINE_ICON = "🤖";

/** The longest a quoted argument may be before it is cut. */
const ARG_MAX = 48;

/** The longest a preview may be before it is cut. */
const PREVIEW_MAX = 160;

/** How many lines an expanded panel may hold before it says what is left. */
const PANEL_MAX_LINES = 60;

/** The human name of a tool. */
export function humanToolName(toolName) {
	if (Object.hasOwn(TOOL_LABELS, toolName)) return TOOL_LABELS[toolName];
	return String(toolName).replace(/^mnexis_/, "").replace(/_/g, " ");
}

/** The icon of a kind of memory. */
export function typeIcon(type) {
	return Object.hasOwn(TYPE_ICONS, type) ? TYPE_ICONS[type] : TYPE_ICONS.fact;
}

/**
 * The kind of one record, the way the panel draws it: what the engine calls
 * it, else the fact it is. A record that carries no kind is drawn as a fact
 * everywhere — the expanded panel and the collapsed line agree.
 */
function kindOf(record) {
	const type = record?.memory_type ?? record?.type;
	return typeof type === "string" && type !== "" ? type : "fact";
}

/** The records a result carries, when it carries them. */
function recordsOf(items) {
	return Array.isArray(items) ? items : [];
}

/** The kinds of a set of records, and how many carry each, in first-seen order. */
function kindsIn(items) {
	const counts = new Map();
	for (const record of items) {
		const type = kindOf(record);
		counts.set(type, (counts.get(type) ?? 0) + 1);
	}
	return counts;
}

/**
 * The kinds of a result with the records that carry each: `🎯×2 📘×1`.
 *
 * The order is the order the result first shows the kind, so the census of a
 * ranked answer leads with the kind that ranked first.
 */
function kindCensus(items) {
	return [...kindsIn(recordsOf(items))]
		.map(([type, count]) => `${typeIcon(type)}×${count}`)
		.join(" ");
}

/**
 * The kinds of a result, each one named once: `🎯`, or `🧠 🎯`.
 *
 * A history is the revisions of one memory, so its count is already the count
 * of revisions: naming the kind is all the line needs to say.
 */
function kindsPresent(items) {
	return [...kindsIn(recordsOf(items)).keys()].map((type) => typeIcon(type)).join(" ");
}

/** A summary line with the kinds of its records on it, when it has any. */
function withKinds(line, kinds) {
	return kinds === "" ? line : `${line} · ${kinds}`;
}

/** The first eight characters of a uuid, the way a person says it. */
export function shortUuid(uuid) {
	const text = String(uuid ?? "").trim();
	return text === "" ? "" : `#${text.slice(0, 8)}`;
}

/** One line of text, cut at a limit. */
export function truncate(value, max) {
	const text = String(value ?? "")
		.replace(/\s+/g, " ")
		.trim();
	if (text.length <= max) return text;
	return `${text.slice(0, Math.max(0, max - 1))}…`;
}

/** The argument worth showing for a call, or nothing. */
export function compactArg(toolName, args = {}) {
	const keys = Object.hasOwn(ARG_KEYS, toolName) ? ARG_KEYS[toolName] : [];
	for (const key of keys) {
		const value = args?.[key];
		if (value === undefined || value === null || value === "") continue;
		if (key === "uuid") return shortUuid(value);
		if (key === "session_id") return truncate(value, 16);
		if (typeof value === "number" || typeof value === "boolean") return String(value);
		return `“${truncate(value, ARG_MAX)}”`;
	}
	return "";
}

/** The line pi draws when the tool is called. */
export function callLines(toolName, args = {}) {
	const label = humanToolName(toolName);
	const arg = compactArg(toolName, args);
	return arg === "" ? [`${ENGINE_ICON} mnexis ${label}`] : [`${ENGINE_ICON} mnexis ${label} · ${arg}`];
}

/** The line pi draws when the answer comes back, collapsed. */
export function summaryLine(toolName, result) {
	if (result?.isError) {
		return `↳ ✗ ${truncate(errorText(result), 80)}`;
	}
	const details = result?.details ?? {};
	switch (toolName) {
		case "mnexis_show": {
			const memory = details.memory ?? {};
			const type = kindOf(memory);
			return `↳ ✓ ${typeIcon(type)} ${type} · rev ${memory.revision ?? "?"} · ${memory.namespace ?? "?"}`;
		}
		case "mnexis_list":
			return withKinds(
				`↳ ✓ ${counted(details.count ?? 0, "memory", "memories")}`,
				kindCensus(details.memories ?? details.hits),
			);
		case "mnexis_search":
			return withKinds(
				`↳ ✓ ${counted(details.count ?? 0, "result")}`,
				kindCensus(details.memories ?? details.hits),
			);
		case "mnexis_recall":
			return `↳ ✓ ${counted(details.memories ?? 0, "memory", "memories")}`;
		case "mnexis_history":
			return withKinds(
				`↳ ✓ ${counted(details.count ?? 0, "revision")}`,
				kindsPresent(details.items),
			);
		case "mnexis_search_prompts":
			return `↳ ✓ ${counted(details.count ?? 0, "prompt")}`;
		case "mnexis_context": {
			const tokens = details.tokens_used;
			const count = details.count ?? 0;
			const line =
				tokens === null || tokens === undefined
					? `↳ ✓ ${counted(count, "memory", "memories")}`
					: `↳ ✓ ${counted(count, "memory", "memories")} · ${tokens} tokens`;
			return withKinds(line, kindCensus(details.memories ?? details.hits));
		}
		case "mnexis_workspace": {
			const workspace = details.workspace ?? {};
			const count = details.status?.memory_count;
			const line =
				typeof count === "number"
					? `↳ ✓ ${workspace.namespace ?? "workspace"} · ${count} memories`
					: `↳ ✓ ${workspace.namespace ?? "workspace"}`;
			return withKinds(line, kindCensus(details.status?.recent));
		}
		case "mnexis_save": {
			// The kind the record landed with, as the answer names it: the
			// engine resolves the type when the caller names none, so a save
			// that arrived untyped reads as the fact it was stored as.
			const type = kindOf(details);
			const saved = `↳ ✓ saved · ${typeIcon(type)} ${type}`;
			return details.title ? `${saved} · ${truncate(details.title, 40)}` : saved;
		}
		case "mnexis_supersede":
			return "↳ ✓ superseded";
		case "mnexis_suggest_topic_key":
			return `↳ ✓ ${details.topic_key ?? "no key"}`;
		case "mnexis_session_start":
			return "↳ ✓ started";
		case "mnexis_session_end":
			return "↳ ✓ ended";
		case "mnexis_save_prompt":
			return "↳ ✓ prompt saved";
		default:
			return "↳ ✓ done";
	}
}

/** The lines pi draws when the answer comes back, expanded. */
export function resultLines(toolName, result, options = {}) {
	if (options.isPartial) {
		// UX7: a partial answer used to replace the whole drawing with one
		// ellipsis, so the call the human was waiting on vanished. The call
		// line stays and the pending state is appended to it. pi draws its own
		// call line above this one under the default shell (UX5), so the two
		// agree; the argument is repeated on purpose, because a host that draws
		// only the result must still say what is running. T7's visual check is
		// where the double line is judged.
		return [`${callLines(toolName, options.args ?? {})[0]} …`];
	}
	if (result?.isError || options.isError) {
		return [`↳ ✗ ${truncate(errorText(result), 200)}`];
	}
	if (!options.expanded) return [summaryLine(toolName, result)];

	const details = result?.details ?? {};
	const lines = [];
	switch (toolName) {
		case "mnexis_show":
			lines.push(...memoryLines(details.memory ?? {}));
			break;
		case "mnexis_history":
			lines.push(...revisionLines(details.items ?? []));
			break;
		case "mnexis_list":
		case "mnexis_search":
			lines.push(...itemLines(details.memories ?? details.hits ?? []));
			break;
		case "mnexis_context":
			lines.push(...itemLines(details.memories ?? details.hits ?? []));
			lines.push(...coverageLines(details.coverage));
			lines.push(...stateLines(details.session_state));
			break;
		case "mnexis_recall":
			lines.push(...itemLines(recallItems(details)));
			break;
		case "mnexis_workspace":
			lines.push(...workspaceLines(details));
			break;
		default:
			lines.push(...textLines(result));
			break;
	}
	return lines;
}

/** The footer's own words for what a tool is doing: `show…`, `show · ✓`. */
export function statusLabel(toolName, result) {
	const label = humanToolName(toolName);
	if (result === undefined) return `${label}…`;
	if (result?.isError) return `${label} · ✗`;
	return `${label} · ✓`;
}

/** The status the footer carries while the tool runs, and after. */
export function statusText(toolName, result) {
	return `${ENGINE_ICON} mnexis · ${statusLabel(toolName, result)}`;
}

/** The renderers pi draws a tool with. */
export function renderers(toolName) {
	// UX5: no `renderShell`. The tool used to opt out of pi's shell, which is
	// what draws the error background, the output padding and the expand
	// keyHint; the Panel is the result's content and the shell is pi's. The
	// Panel draws text and nothing else — no background, no padding — so the
	// default shell applies cleanly. The visual confirmation is T7's
	// (odd/tasks/ext-ts-rewrite.md, UX5).
	return {
		renderCall(args) {
			return new Panel(callLines(toolName, args));
		},
		renderResult(result, options, _theme, context) {
			// pi hands the renderer the content and the details, never the
			// error flag: that lives in the render context, the fourth
			// argument. Without it a failed call would draw a tick. The render
			// context also carries the call's own `args`, which is what the
			// partial line needs to keep naming the call.
			const isError = Boolean(options?.isError || context?.isError);
			return new Panel(
				resultLines(toolName, result, { ...options, isError, args: context?.args }),
				{ maxLines: PANEL_MAX_LINES },
			);
		},
	};
}

/**
 * Wrap a tool's `execute` so the footer carries the memory engine's status
 * while it runs and what it answered after.
 *
 * `footer` is the session's own footer (UX2): when the session has one, the
 * status is written through it, so the adoption counters and the running call
 * share the one `mnexis` key instead of overwriting each other.
 */
export function withStatus(toolName, execute, footer = null) {
	return async function wrapped(toolCallId, params, signal, onUpdate, ctx) {
		const ui = ctx && ctx.ui;
		const setStatus = ui && typeof ui.setStatus === "function" ? ui.setStatus.bind(ui) : null;
		const write = (result) => {
			if (!setStatus) return;
			if (footer && typeof footer.line === "function") {
				setStatus("mnexis", footer.line(statusLabel(toolName, result)));
				return;
			}
			setStatus("mnexis", statusText(toolName, result));
		};
		write(undefined);
		try {
			const result = await execute(toolCallId, params, signal, onUpdate, ctx);
			write(result);
			return result;
		} catch (error) {
			write({ isError: true });
			throw error;
		}
	};
}

/**
 * The smallest component pi accepts: lines of text, wrapped to the width it
 * is given. pi measures visible columns, not string length, so an emoji
 * counts as the two columns it takes on a terminal.
 */
export class Panel {
	constructor(lines, options = {}) {
		this.lines = Array.isArray(lines) ? lines.map((line) => String(line ?? "")) : [String(lines ?? "")];
		this.maxLines = Number(options.maxLines) > 0 ? Number(options.maxLines) : null;
		// A style is applied AFTER the line is wrapped, so the width math
		// stays on the plain text and a terminal's escape codes never count
		// as columns.
		this.style = typeof options.style === "function" ? options.style : null;
	}

	render(width) {
		const limit = Math.max(1, Number(width) || 80);
		const rendered = this.lines.flatMap((line) => wrapLine(line, limit));
		let kept;
		if (this.maxLines === null || rendered.length <= this.maxLines) {
			kept = rendered.length === 0 ? [""] : rendered;
		} else {
			kept = rendered.slice(0, this.maxLines - 1);
			kept.push(`… ${rendered.length - (this.maxLines - 1)} more lines`);
		}
		return this.style ? kept.map((line) => String(this.style(line))) : kept;
	}

	invalidate() {}
}

/** The visible width of a string, in terminal columns. */
export function displayWidth(text) {
	return graphemes(String(text ?? "")).reduce((sum, cluster) => sum + clusterWidth(cluster), 0);
}

/** The width of one grapheme cluster. */
function clusterWidth(cluster) {
	// A sequence joined by ZWJ is one glyph on the terminal, even when it is
	// made of three people.
	if (cluster.includes("\u200d")) return 2;
	let width = 0;
	for (const character of cluster) {
		const code = character.codePointAt(0);
		if (code >= 0x0300 && code <= 0x036f) continue;
		if (code === 0xfe0f) continue;
		if (code < 0x20) continue;
		if (code >= 0x1f000) {
			width += 2;
			continue;
		}
		if (code >= 0x2600 && code <= 0x2bff) {
			width += 2;
			continue;
		}
		if (isWideCodePoint(code)) {
			width += 2;
			continue;
		}
		width += 1;
	}
	return width;
}

/** The code points a terminal draws two columns wide. */
function isWideCodePoint(code) {
	return (
		(code >= 0x1100 && code <= 0x115f) ||
		(code >= 0x2e80 && code <= 0x303e) ||
		(code >= 0x3041 && code <= 0x33ff) ||
		(code >= 0x3400 && code <= 0x4dbf) ||
		(code >= 0x4e00 && code <= 0x9fff) ||
		(code >= 0xa000 && code <= 0xa4cf) ||
		(code >= 0xac00 && code <= 0xd7a3) ||
		(code >= 0xf900 && code <= 0xfaff) ||
		(code >= 0xfe30 && code <= 0xfe6f) ||
		(code >= 0xff00 && code <= 0xff60) ||
		(code >= 0xffe0 && code <= 0xffe6) ||
		(code >= 0x20000 && code <= 0x3fffd)
	);
}

/** The grapheme clusters of a string, so a wrapping never splits an emoji. */
function graphemes(text) {
	if (typeof Intl !== "undefined" && typeof Intl.Segmenter === "function") {
		const segmenter = new Intl.Segmenter("en", { granularity: "grapheme" });
		return Array.from(segmenter.segment(text), (entry) => entry.segment);
	}
	return Array.from(text);
}

/** Wrap one line to a width, breaking on spaces and never mid-emoji. */
export function wrapLine(text, width) {
	const limit = Math.max(1, width);
	const source = String(text ?? "");
	if (source === "") return [""];
	if (displayWidth(source) <= limit) return [source];

	const lines = [];
	let current = "";
	let currentWidth = 0;
	let lastSpace = -1;

	for (const cluster of graphemes(source)) {
		const size = clusterWidth(cluster);
		if (cluster === " ") {
			lastSpace = current.length;
		}
		if (currentWidth + size > limit && current !== "") {
			if (lastSpace > 0) {
				lines.push(current.slice(0, lastSpace).trimEnd());
				current = current.slice(lastSpace + 1);
			} else {
				lines.push(current);
				current = "";
			}
			currentWidth = displayWidth(current);
			lastSpace = current.includes(" ") ? current.lastIndexOf(" ") : -1;
			if (cluster === " ") continue;
		}
		current += cluster;
		currentWidth += size;
	}
	if (current !== "") lines.push(current);
	return lines.length === 0 ? [""] : lines;
}

/** A count with its noun: one memory, two memories. */
function counted(count, singular, plural = `${singular}s`) {
	return `${count} ${count === 1 ? singular : plural}`;
}

/** The lines of a memory: what it is, then what it says. */
function memoryLines(memory) {
	const type = memory.memory_type ?? "fact";
	const lines = [`${typeIcon(type)} ${type} · ${shortUuid(memory.uuid)}`];
	if (memory.title) lines.push(memory.title);
	lines.push("");
	lines.push(`state       ${memory.state ?? "—"}`);
	lines.push(`revision    ${memory.revision ?? "—"}`);
	lines.push(`namespace   ${memory.namespace ?? "—"}`);
	if (memory.created_at) lines.push(`created     ${memory.created_at}`);
	if (memory.updated_at && memory.updated_at !== memory.created_at) {
		lines.push(`updated     ${memory.updated_at}`);
	}
	if (memory.assertion_basis) lines.push(`basis       ${memory.assertion_basis}`);
	if (memory.content) {
		lines.push("");
		lines.push(String(memory.content));
	}
	return lines;
}

/**
 * One line per memory: what it is, and a few words of what it says.
 *
 * A record that carries its whole content shows it — the door's items and the
 * state block's resident records do, and the drawing is the only view the
 * human has.
 */
function itemLines(items) {
	if (!Array.isArray(items) || items.length === 0) return ["(nothing)"];
	return items.flatMap((item) => {
		const type = item.memory_type ?? item.type ?? "fact";
		const uuid = shortUuid(item.uuid);
		const head = `${typeIcon(type)} ${type}${uuid ? ` · ${uuid}` : ""}`;
		if (typeof item.content === "string" && item.content !== "") {
			return [head, ...indentLines(item.content)];
		}
		const text = item.title || item.preview || "";
		return text === "" ? [head] : [`${head}  ${truncate(text, PREVIEW_MAX)}`];
	});
}

/** One line per revision of a memory, with why it changed and what it said. */
function revisionLines(items) {
	if (!Array.isArray(items) || items.length === 0) return ["(no revisions)"];
	return items.flatMap((item, index) => {
		const revision = item.revision ?? index + 1;
		const state = item.state ?? "—";
		const when = item.changed_at ?? item.updated_at ?? item.created_at ?? "";
		const who = item.changed_by ?? item.actor ?? item.assertion_basis ?? "";
		const lines = [
			`🔁 rev ${revision} · ${state}${when ? ` · ${when}` : ""}${who ? ` · ${who}` : ""}`,
		];
		if (item.change_reason) lines.push(`   reason: ${item.change_reason}`);
		if (item.content) lines.push(...indentLines(item.content));
		return lines;
	});
}

/** The text of a record, one line per line, indented under its heading. */
function indentLines(text, prefix = "   ") {
	return String(text)
		.split("\n")
		.map((line) => `${prefix}${line}`);
}

/** The workspace answer: where this session writes, and what is there. */
function workspaceLines(details) {
	const workspace = details.workspace ?? {};
	const status = details.status ?? {};
	const lines = [`${ENGINE_ICON} mnexis · ${workspace.namespace ?? "workspace"}`];
	const recent = Array.isArray(status.recent) ? status.recent : [];
	if (recent.length > 0) {
		lines.push(...itemLines(recent));
	}
	return lines;
}

/** The memories of a recall: the priced ones when the engine sends them. */
function recallItems(details) {
	const recall = details.recall ?? {};
	if (Array.isArray(recall.previews) && recall.previews.length > 0) return recall.previews;
	if (Array.isArray(recall.memories_in_window)) return recall.memories_in_window;
	return [];
}

/**
 * The session state the door's answer carries.
 *
 * The block is the live state with the content that fits its budget, plus the
 * index of the live records that did not fit. The human reads the same block
 * the agent does: the records whole, and the index as the list of identities
 * it is — so what the budget left out is visible and one `show` away.
 *
 * An answer from an engine that predates the block has no `session_state`, and
 * then nothing is drawn: the panel is exactly the one it always was.
 */
function stateLines(block) {
	if (!block || typeof block !== "object") return [];
	const records = Array.isArray(block.records) ? block.records : [];
	const index = Array.isArray(block.index) ? block.index : [];
	const session = block.session && typeof block.session === "object" ? block.session : null;
	if (records.length === 0 && index.length === 0 && session === null) return [];

	const resident = typeof block.resident_tokens === "number" ? block.resident_tokens : null;
	const budget = typeof block.budget === "number" ? block.budget : null;
	const held = counted(records.length, "record");
	const lines = [
		resident === null || budget === null
			? `session state · ${held}`
			: `session state · ${held} · ${resident} of ${budget} tokens`,
	];
	if (records.length > 0) lines.push(...itemLines(records));
	if (index.length > 0) {
		lines.push(`indexed · ${counted(index.length, "record")} did not fit the budget`);
		lines.push(...index.map(indexLine));
	}
	if (session) {
		const id = typeof session.id === "string" && session.id !== "" ? ` · ${session.id}` : "";
		const ended =
			typeof session.ended_at === "string" && session.ended_at !== ""
				? ` · ended ${session.ended_at}`
				: "";
		lines.push(`previous session${id}${ended}`);
		if (typeof session.summary === "string" && session.summary !== "") {
			lines.push(...indentLines(session.summary));
		}
	}
	return lines;
}

/**
 * What the answer did not include: the candidates the budget could not keep.
 *
 * The human reads the same coverage the agent reads: the count, and the
 * records with the size that explains why they did not fit.
 */
function coverageLines(coverage) {
	if (!coverage || typeof coverage !== "object") return [];
	const items = Array.isArray(coverage.left_out_items) ? coverage.left_out_items : [];
	const leftOut = typeof coverage.left_out === "number" ? coverage.left_out : items.length;
	if (leftOut === 0 && items.length === 0) return [];

	const lines = [`not included · ${counted(leftOut, "candidate")} did not fit the budget`];
	for (const entry of items) {
		const type = entry?.memory_type ?? "fact";
		const uuid = shortUuid(entry?.uuid);
		const bytes = typeof entry?.bytes === "number" ? entry.bytes : 0;
		const said = [entry?.title, uuid ? `· ${uuid}` : "", `· ${bytes} bytes`]
			.filter((part) => typeof part === "string" && part !== "")
			.join(" ");
		lines.push(said === "" ? typeIcon(type) : `${typeIcon(type)} ${truncate(said, PREVIEW_MAX)}`);
	}
	return lines;
}

/** One indexed record: what it is, how to name it, and when it was written. */
function indexLine(entry) {
	const type = entry?.memory_type ?? "project_state";
	const uuid = shortUuid(entry?.uuid);
	const head = `${typeIcon(type)} ${type}${uuid ? ` · ${uuid}` : ""}`;
	const said = [entry?.title, entry?.created_at]
		.filter((part) => typeof part === "string" && part !== "")
		.join(" · ");
	return said === "" ? head : `${head}  ${truncate(said, PREVIEW_MAX)}`;
}

/** The text the tool returned, as lines. */
function textLines(result) {
	const content = result?.content ?? [];
	const text = content
		.filter((entry) => entry && entry.type === "text" && typeof entry.text === "string")
		.map((entry) => entry.text)
		.join("\n");
	return text === "" ? ["(empty)"] : text.split("\n");
}

/** The error the tool returned, as one line. */
function errorText(result) {
	const content = result?.content ?? [];
	const text = content
		.filter((entry) => entry && entry.type === "text" && typeof entry.text === "string")
		.map((entry) => entry.text)
		.join(" ");
	return text.trim() || String(result?.details?.error ?? "error");
}

/**
 * The journal the human reads (ext-ts-rewrite UX1, UX3, UX4).
 *
 * The extension writes four kinds of session entry and one status card. An
 * entry with no registered renderer draws nothing in the chat (it still lives
 * in the session file), so every kind this extension writes has a renderer
 * here: one dim line naming the kind and the fact it holds when collapsed,
 * and the entry's own data when expanded. The line is built from the entry's
 * `action` and `step` — the variants the reliability work added — and never
 * from an assumed `skipped`: a close that was abandoned, a start the engine
 * refused, and an end the engine never knew are three different facts.
 */

/** The entry kinds the extension writes, each with a renderer. */
export const JOURNAL_KINDS = [
	"mnexis-store-recovery",
	"mnexis-prompt-capture",
	"mnexis-session-close",
	"mnexis-compaction",
	"mnexis-status",
];

/** How each journal kind is said in one word. */
const JOURNAL_LABELS = {
	"mnexis-store-recovery": "store recovery",
	"mnexis-prompt-capture": "prompt capture",
	"mnexis-session-close": "session close",
	"mnexis-compaction": "compaction",
	"mnexis-status": "status",
};

/** A count with its noun, in the journal's own words. */
function journalCount(count, singular, plural = `${singular}s`) {
	return `${count} ${count === 1 ? singular : plural}`;
}

/** The head of every journal line: the engine and the kind. */
function journalHead(customType) {
	const label = Object.hasOwn(JOURNAL_LABELS, customType)
		? JOURNAL_LABELS[customType]
		: String(customType ?? "journal").replace(/^mnexis-/, "").replace(/-/g, " ");
	return `${ENGINE_ICON} mnexis · ${label}`;
}

/** What a store-recovery entry did: the repair, or the step it could not finish. */
function recoveryFact(data) {
	const action = typeof data.action === "string" ? data.action : "";
	if (action === "supersede") {
		const value = data.value === undefined || data.value === null ? "" : ` (value ${data.value})`;
		return `repaired a duplicate${value}`;
	}
	if (action === "failed") {
		const step = typeof data.step === "string" && data.step !== "" ? data.step : "the repair";
		return `could not finish at ${step}${said(data.error)}`;
	}
	return action === "" ? "noted" : action;
}

/** What a prompt-capture entry did, and how large the message was. */
function captureFact(data) {
	const action = typeof data.action === "string" && data.action !== "" ? data.action : "noted";
	const size = typeof data.bytes === "number" ? ` · ${data.bytes} B` : "";
	const code = typeof data.exit_code === "number" ? ` · exit ${data.exit_code}` : "";
	return `${action}${size}${code}`;
}

/**
 * What a session-close entry says.
 *
 * The journal carries `action` (`noted` or `abandoned`) and `step` (`start`,
 * `end`, `save`, `hook`), plus the engine answers worth naming (`not_registered`
 * for exit 3, `already_ended` for a resume of a closed session). The line
 * reads those variants; it never assumes the old `skipped`.
 */
function closeFact(data) {
	const action = typeof data.action === "string" ? data.action : "";
	const step = typeof data.step === "string" ? data.step : "";
	if (action === "abandoned") {
		return `${step === "" ? "the close" : step} was abandoned${said(data.reason)}`;
	}
	if (step === "start") {
		if (data.already_ended === true) {
			return "the engine holds this session as already ended";
		}
		return `the start was refused${exit(data.exit_code)}${said(data.error)}`;
	}
	if (step === "end") {
		if (data.not_registered === true) {
			return "the engine never registered this session";
		}
		return `the end was refused${exit(data.exit_code)}${said(data.error)}`;
	}
	if (step === "save") {
		return `the estado al cierre was not saved${exit(data.exit_code)}${said(data.error)}`;
	}
	if (step === "hook") {
		return `the close failed${said(data.error)}`;
	}
	return action === "" ? "noted" : action;
}

/** What a compaction entry says: the host failed, or the store refused it. */
function compactionFact(data) {
	const action = typeof data.action === "string" ? data.action : "";
	if (action === "failed") {
		const retry = data.will_retry === true ? " (it will retry)" : "";
		return `the host could not compact${retry}${said(data.error)}`;
	}
	return `the summary was not stored${exit(data.exit_code)}${said(data.error)}`;
}

/** What the `/mnexis` card says when collapsed: where, and how much. */
function statusFact(data) {
	const namespace =
		typeof data.namespace === "string" && data.namespace !== "" ? data.namespace : "unbound";
	const memories =
		typeof data.memories === "number" ? journalCount(data.memories, "memory", "memories") : "memories unknown";
	return `${namespace} · ${memories}`;
}

/** `:` and a short reason, when the entry carries one. */
function said(value) {
	const text = typeof value === "string" ? value.trim() : "";
	return text === "" ? "" : `: ${truncate(text, 80)}`;
}

/** ` (exit N)`, when the entry carries one. */
function exit(code) {
	return typeof code === "number" ? ` (exit ${code})` : "";
}

/** The one line a journal entry shows while it is collapsed. */
export function journalSummary(customType, data = {}) {
	const head = journalHead(customType);
	switch (customType) {
		case "mnexis-store-recovery":
			return `${head} · ${recoveryFact(data)}`;
		case "mnexis-prompt-capture":
			return `${head} · ${captureFact(data)}`;
		case "mnexis-session-close":
			return `${head} · ${closeFact(data)}`;
		case "mnexis-compaction":
			return `${head} · ${compactionFact(data)}`;
		case "mnexis-status":
			return `${head} · ${statusFact(data)}`;
		default:
			return head;
	}
}

/** The entry's data as the card draws it: one aligned label per field. */
function journalField(label, value) {
	return `${label.padEnd(14)}${value}`;
}

/** A value as one line: text is kept, anything structured is its JSON. */
function detailValue(value) {
	if (value === null || value === undefined || value === "") return "";
	if (typeof value === "string") return truncate(value, 200);
	if (typeof value === "number" || typeof value === "boolean") return String(value);
	return truncate(JSON.stringify(value), 200);
}

/** The data of a journal entry, one field per line. */
function genericDetail(data) {
	const lines = [];
	for (const [key, value] of Object.entries(data ?? {})) {
		// The timestamp and the internal kind are not facts the human reads.
		if (key === "ts" || key === "kind") continue;
		const text = detailValue(value);
		if (text === "") continue;
		lines.push(journalField(key, text));
	}
	return lines.length === 0 ? ["(no detail)"] : lines;
}

/** A list of names, as the card writes it. */
function nameList(value) {
	if (!Array.isArray(value) || value.length === 0) return "(none)";
	return value.map((name) => String(name)).join(", ");
}

/**
 * The `/mnexis` card, expanded: every field the command gathered, in the
 * order the question is asked — where, what may be read and written, who the
 * session is, how much is there, how the session used it, whether the
 * question was asked.
 */
function statusCardDetail(data) {
	const counters = data.counters && typeof data.counters === "object" ? data.counters : {};
	const lines = [
		journalField("workspace", data.workspace ? String(data.workspace) : "(unknown)"),
		journalField("namespace", data.namespace ? String(data.namespace) : "(unbound)"),
		journalField("bound", data.bound === true ? "yes" : "no"),
		journalField("read", nameList(data.read_namespaces)),
		journalField("write", nameList(data.write_namespaces)),
		journalField("session", data.session ? String(data.session) : "(none)"),
		journalField("session state", data.session_state ? String(data.session_state) : "unknown"),
	];
	if (typeof data.session_ended_at === "string" && data.session_ended_at !== "") {
		lines.push(journalField("ended", data.session_ended_at));
	}
	lines.push(
		journalField(
			"memories",
			typeof data.memories === "number"
				? `${data.memories}${data.memories_read_all === true ? " (profile scope)" : ""}`
				: "(unknown)",
		),
		journalField(
			"prompts",
			`${typeof data.prompts_captured === "number" ? data.prompts_captured : 0} captured this session`,
		),
		journalField("counters", countersText(counters)),
		journalField("nudge", data.nudge === "sent" ? "sent" : "open"),
	);
	const engine = data.engine && typeof data.engine === "object" ? data.engine.error : "";
	if (typeof engine === "string" && engine !== "") {
		lines.push(journalField("engine", truncate(engine, 200)));
	}
	return lines;
}

/** The expanded lines of a journal entry: its own data, in its own shape. */
export function journalDetail(customType, data = {}) {
	return customType === "mnexis-status" ? statusCardDetail(data) : genericDetail(data);
}

/**
 * The data a journal entry carries, however the caller held it: the renderer
 * is handed the whole entry, and a test may hand the data itself.
 */
function journalData(entry) {
	if (entry && typeof entry === "object" && entry.data && typeof entry.data === "object") {
		return entry.data;
	}
	return entry && typeof entry === "object" ? entry : {};
}

/** The lines of one journal entry: one when collapsed, its data when expanded. */
export function journalLines(customType, entry, options = {}) {
	const data = journalData(entry);
	const summary = journalSummary(customType, data);
	if (!options.expanded) return [summary];
	return [summary, "", ...journalDetail(customType, data)];
}

/** A dim line, when the theme can say so. */
function dim(theme, text) {
	return theme && typeof theme.fg === "function" ? theme.fg("dim", text) : text;
}

/**
 * The renderer pi calls for one journal kind.
 *
 * Collapsed, the line is dim: it is the engine's own record, not the
 * conversation. Expanded, the entry's data is the card.
 */
export function journalRenderer(customType, theme = null) {
	return (entry, options, callTheme = theme) => {
		const expanded = Boolean(options?.expanded);
		return new Panel(journalLines(customType, entry, { expanded }), {
			style: expanded ? null : (text) => dim(callTheme, text),
		});
	};
}

/** The one line the settle nudge collapses to (UX4). */
export const NUDGE_HEADLINE = `${ENGINE_ICON} mnexis · asked the agent to save — once per session`;

/** The text a custom message carries, as lines. */
function messageLines(message) {
	const content = message?.content;
	if (typeof content === "string") return content.split("\n");
	if (Array.isArray(content)) {
		return content
			.filter((part) => part && part.type === "text" && typeof part.text === "string")
			.flatMap((part) => part.text.split("\n"));
	}
	return [""];
}

/** The lines of the settle nudge: one when collapsed, the question when expanded. */
export function nudgeLines(message, options = {}) {
	if (!options.expanded) return [NUDGE_HEADLINE];
	return [NUDGE_HEADLINE, "", ...messageLines(message)];
}

/**
 * The renderer pi calls for the settle nudge (UX4).
 *
 * The nudge used to be `display: false`, so the human could not tell the
 * extension's own question from something the agent made up. Now it is drawn,
 * collapsed to the fact that it was asked, and expanded to its text.
 */
export function nudgeRenderer(theme = null) {
	return (message, options, callTheme = theme) => {
		const expanded = Boolean(options?.expanded);
		return new Panel(nudgeLines(message, { expanded }), {
			style: expanded ? null : (text) => dim(callTheme, text),
		});
	};
}

/** The counter columns, as one phrase: `ctx 1 · search 2 · save 1`. */
export function countersText(counts = {}) {
	const count = (value) => (Number.isFinite(value) && value > 0 ? Math.trunc(value) : 0);
	return `ctx ${count(counts.ctx)} · search ${count(counts.search)} · save ${count(counts.save)}`;
}

/**
 * The footer's counter line (UX2): always visible, for the whole session.
 *
 * `ctx` is a call to the door or to the workspace, `search` a call to search
 * or show, and `save` a save that landed. The line is the instrument the
 * adoption question never had: a session that read nothing said nothing at
 * all before it.
 */
export function counterLine(counts = {}) {
	return `${ENGINE_ICON} mnexis · ${countersText(counts)}`;
}

/**
 * The session's footer (UX2): the counters, plus whatever the running tool is
 * doing, on the one key the tools already write.
 *
 * One key on purpose: pi's footer holds one line per key, so a second key
 * would either overwrite this one or hide beside it. `line(status)` composes
 * the status into the counter line, which is how a tool run and a session's
 * adoption stay visible at the same time.
 */
export function createFooter() {
	const counts = { ctx: 0, search: 0, save: 0 };
	let status = "";

	const text = () => (status === "" ? counterLine(counts) : `${counterLine(counts)} · ${status}`);

	const write = (ui) => {
		if (ui && typeof ui.setStatus === "function") {
			ui.setStatus("mnexis", text());
		}
	};

	return {
		/** The counters as they stand. */
		counts: () => ({ ...counts }),
		/** The line as it would be written now. */
		text,
		/** Count one call, and show the line. */
		count(family, ui = null) {
			if (!Object.hasOwn(counts, family)) return;
			counts[family] += 1;
			write(ui);
		},
		/** Start the counters again, for the session that is starting. */
		reset() {
			counts.ctx = 0;
			counts.search = 0;
			counts.save = 0;
			status = "";
		},
		/** Show the line, with no tool status on it. */
		show(ui) {
			status = "";
			write(ui);
		},
		/** Compose a tool status into the line, and answer it. */
		line(nextStatus) {
			status = typeof nextStatus === "string" ? nextStatus : "";
			return text();
		},
	};
}
