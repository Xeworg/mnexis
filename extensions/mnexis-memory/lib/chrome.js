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

/** The icon of every kind of memory — the closed set the agent writes. */
export const TYPE_ICONS = {
	knowledge: "📘",
	decision: "⚖️",
	preference: "⭐",
	procedure: "🧭",
	project_state: "📍",
	episode: "🎞️",
	fact: "🧠",
};

/** The engine's own mark, so a session with two memory engines stays readable. */
const ENGINE_ICON = "🗃️";

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
			const type = memory.memory_type ?? "fact";
			return `↳ ✓ ${type} · rev ${memory.revision ?? "?"} · ${memory.namespace ?? "?"}`;
		}
		case "mnexis_list":
			return `↳ ✓ ${details.count ?? 0} memories`;
		case "mnexis_search":
			return `↳ ✓ ${details.count ?? 0} results`;
		case "mnexis_recall":
			return `↳ ✓ ${details.memories ?? 0} memories`;
		case "mnexis_history":
			return `↳ ✓ ${details.count ?? 0} revisions`;
		case "mnexis_search_prompts":
			return `↳ ✓ ${details.count ?? 0} prompts`;
		case "mnexis_context": {
			const tokens = details.tokens_used;
			const count = details.count ?? 0;
			return tokens === null || tokens === undefined
				? `↳ ✓ ${count} memories`
				: `↳ ✓ ${count} memories · ${tokens} tokens`;
		}
		case "mnexis_workspace": {
			const workspace = details.workspace ?? {};
			const count = details.status?.memory_count;
			return typeof count === "number"
				? `↳ ✓ ${workspace.namespace ?? "workspace"} · ${count} memories`
				: `↳ ✓ ${workspace.namespace ?? "workspace"}`;
		}
		case "mnexis_save":
			return `↳ ✓ saved${details.title ? ` · ${truncate(details.title, 40)}` : ""}`;
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
	if (options.isPartial) return ["…"];
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
		case "mnexis_context":
			lines.push(...itemLines(details.memories ?? details.hits ?? []));
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

/** The status the footer carries while the tool runs, and after. */
export function statusText(toolName, result) {
	const label = humanToolName(toolName);
	if (result === undefined) return `${ENGINE_ICON} mnexis · ${label}…`;
	if (result?.isError) return `${ENGINE_ICON} mnexis · ${label} · ✗`;
	return `${ENGINE_ICON} mnexis · ${label} · ✓`;
}

/** The renderers pi draws a tool with. */
export function renderers(toolName) {
	return {
		renderShell: "self",
		renderCall(args) {
			return new Panel(callLines(toolName, args));
		},
		renderResult(result, options) {
			return new Panel(resultLines(toolName, result, options), { maxLines: PANEL_MAX_LINES });
		},
	};
}

/**
 * Wrap a tool's `execute` so the footer carries the memory engine's status
 * while it runs and what it answered after.
 */
export function withStatus(toolName, execute) {
	return async function wrapped(toolCallId, params, signal, onUpdate, ctx) {
		const ui = ctx && ctx.ui;
		const setStatus = ui && typeof ui.setStatus === "function" ? ui.setStatus.bind(ui) : null;
		if (setStatus) setStatus("mnexis", statusText(toolName));
		try {
			const result = await execute(toolCallId, params, signal, onUpdate, ctx);
			if (setStatus) setStatus("mnexis", statusText(toolName, result));
			return result;
		} catch (error) {
			if (setStatus) setStatus("mnexis", statusText(toolName, { isError: true }));
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
	}

	render(width) {
		const limit = Math.max(1, Number(width) || 80);
		const rendered = this.lines.flatMap((line) => wrapLine(line, limit));
		if (this.maxLines === null || rendered.length <= this.maxLines) {
			return rendered.length === 0 ? [""] : rendered;
		}
		const kept = rendered.slice(0, this.maxLines - 1);
		kept.push(`… ${rendered.length - (this.maxLines - 1)} more lines`);
		return kept;
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
	if (memory.content) {
		lines.push("");
		lines.push(String(memory.content));
	}
	return lines;
}

/** One line per memory: what it is, and a few words of what it says. */
function itemLines(items) {
	if (!Array.isArray(items) || items.length === 0) return ["(nothing)"];
	return items.map((item) => {
		const type = item.memory_type ?? item.type ?? "fact";
		const uuid = shortUuid(item.uuid);
		const text = item.title || item.preview || item.content || "";
		const head = `${typeIcon(type)} ${type}${uuid ? ` · ${uuid}` : ""}`;
		return text === "" ? head : `${head}  ${truncate(text, PREVIEW_MAX)}`;
	});
}

/** One line per revision of a memory. */
function revisionLines(items) {
	if (!Array.isArray(items) || items.length === 0) return ["(no revisions)"];
	return items.map((item, index) => {
		const revision = item.revision ?? index + 1;
		const state = item.state ?? "—";
		const when = item.changed_at ?? item.updated_at ?? item.created_at ?? "";
		const who = item.changed_by ?? item.actor ?? item.assertion_basis ?? "";
		return `🔁 rev ${revision} · ${state}${when ? ` · ${when}` : ""}${who ? ` · ${who}` : ""}`;
	});
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

/** Keep a panel within its lines, saying what did not fit. */
