#!/usr/bin/env node
/**
 * Build `index.js` — the one file both opencode plugin models load.
 *
 * opencode 1.x discovers plugin FILES and 2.x discovers directories, and the
 * shipped artifact is a single file that imports nothing of its own
 * (RESEARCH.md §1). The sources therefore live under `src/` as plain sections:
 * no section imports another, and this script concatenates them in the order
 * below. `index.js` is generated, never edited by hand.
 *
 *   node src/build.js          # write index.js, then verify it
 *   node src/build.js --check  # verify the file on disk, write nothing
 *   node src/build.js --freeze # record src/baseline.canonical.txt from stdin
 *
 * The verification is the reorganization's own guard. `baseline.canonical.txt`
 * is the pre-reorganization plugin with comments stripped and its code lines
 * sorted, so a check can say what a plain diff cannot: no line and no quoted
 * payload was lost or reworded — the code was only split, re-ordered, and glued
 * with the lines the sections need to share one instance's state.
 */

import { readFileSync, readdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const src = dirname(fileURLToPath(import.meta.url));
const root = dirname(src);
const indexPath = join(root, "index.js");
const baselinePath = join(src, "baseline.canonical.txt");

/** The line of the baseline file that separates its two sections. */
const MARKER_PAYLOADS = "# payloads (one JSON string per line)";

/** The concatenation order: `index.js` is exactly these sections, in order. */
const SECTIONS = [
	"header.js",
	"constants.js",
	"budget.js",
	"cli.js",
	"gate.js",
	"adapters.js",
	"render.js",
	"catalog.js",
	"settle.js",
	"session.js",
	"pointer.js",
	"core.js",
	"entry-v2.js",
	"entry-v1.js",
	"plugin.js",
];

const BANNER = [
	"// GENERATED FILE — do not edit: `node src/build.js` rewrites this file from",
	"// the ordered sections in src/. The shipped artifact is one file, and it",
	"// imports nothing of its own.",
].join("\n");

/** One section's source, without the blank lines that separate it from the next. */
function sectionText(name) {
	return readFileSync(join(src, name), "utf8").replace(/^\n+/, "").replace(/\s+$/, "");
}

/** The file this script writes: the banner, then every section in order. */
function generate() {
	const parts = [BANNER];
	for (const [index, name] of SECTIONS.entries()) {
		const marker = index === 0 ? "" : `// === src/${name} ===\n\n`;
		parts.push(marker + sectionText(name));
	}
	return `${parts.join("\n\n")}\n`;
}

/**
 * The code in `source`, with comments stripped and the pieces that must stay
 * byte-identical collected: every code line (whitespace collapsed, blank lines
 * dropped) and every quoted payload, verbatim.
 */
function scan(source) {
	const lines = [];
	const payloads = [];
	let line = "";
	let index = 0;
	while (index < source.length) {
		const char = source[index];
		const next = source[index + 1];
		if (char === "/" && next === "/") {
			while (index < source.length && source[index] !== "\n") index += 1;
			continue;
		}
		if (char === "/" && next === "*") {
			index += 2;
			while (index < source.length && !(source[index] === "*" && source[index + 1] === "/")) {
				index += 1;
			}
			index += 2;
			continue;
		}
		if (char === "\n") {
			lines.push(line);
			line = "";
			index += 1;
			continue;
		}
		if (char === '"' || char === "'" || char === "`") {
			const quoted = quote(source, index);
			payloads.push(quoted.text);
			line += quoted.text;
			index = quoted.end;
			continue;
		}
		line += char;
		index += 1;
	}
	lines.push(line);
	const canonical = lines
		.map((text) => text.replace(/\s+/g, " ").trim())
		.filter((text) => text !== "");
	return { lines: canonical.sort(), payloads: payloads.sort() };
}

/**
 * One quoted payload, from its opening quote to its closing one. A template
 * literal keeps its `${…}` expressions inside the payload, so a reworded
 * interpolation is a reworded payload too.
 */
function quote(source, start) {
	const opener = source[start];
	let text = opener;
	let index = start + 1;
	let depth = 0;
	while (index < source.length) {
		const char = source[index];
		if (char === "\\") {
			text += char + (source[index + 1] ?? "");
			index += 2;
			continue;
		}
		if (depth === 0 && char === opener) {
			return { text: text + char, end: index + 1 };
		}
		if (opener === "`" && char === "$" && source[index + 1] === "{") {
			depth += 1;
			text += "${";
			index += 2;
			continue;
		}
		if (opener === "`" && depth > 0 && char === "}") {
			depth -= 1;
		}
		text += char;
		index += 1;
	}
	throw new Error(`an unterminated ${opener} payload at offset ${start}`);
}

/**
 * The entries `expected` carries that `actual` does not, counting duplicates:
 * a line that lost one of its two occurrences is a lost line.
 */
function missing(expected, actual) {
	const counts = new Map();
	for (const entry of actual) counts.set(entry, (counts.get(entry) ?? 0) + 1);
	const lacking = [];
	for (const entry of expected) {
		const left = counts.get(entry) ?? 0;
		if (left === 0) lacking.push(entry);
		else counts.set(entry, left - 1);
	}
	return lacking;
}

/**
 * The baseline frozen before the reorganization: the canonical lines, then the
 * quoted payloads exactly as they were (a payload may hold a run of spaces that
 * the line-level comparison collapses, and a reworded payload is the one change
 * this guard must never accept).
 */
function readBaseline() {
	const lines = readFileSync(baselinePath, "utf8").split("\n");
	const split = lines.indexOf(MARKER_PAYLOADS);
	if (split < 0) {
		throw new Error(`${baselinePath} carries no ${MARKER_PAYLOADS} section`);
	}
	return {
		lines: lines.slice(0, split).filter((line) => !line.startsWith("#") && line !== ""),
		payloads: lines.slice(split + 1).filter((line) => line !== "").map((line) => JSON.parse(line)),
	};
}

/** Fail unless the generated file still says everything the baseline said. */
function verify(generated) {
	const frozen = readBaseline();
	const current = scan(generated);
	const lostLines = missing(frozen.lines, current.lines);
	const lostPayloads = missing(frozen.payloads, current.payloads);
	if (lostLines.length > 0 || lostPayloads.length > 0) {
		const report = [
			"the reorganization changed the plugin's code, not just its shape.",
			...lostLines.slice(0, 10).map((line) => `  lost or reworded line: ${line}`),
			...lostPayloads.slice(0, 10).map((text) => `  lost or reworded text: ${text}`),
		];
		throw new Error(report.join("\n"));
	}
	return {
		sections: SECTIONS.length,
		lines: generated.split("\n").length - 1,
		frozenLines: frozen.lines.length,
		added: current.lines.length - frozen.lines.length,
		payloads: frozen.payloads.length,
	};
}

/** Fail unless every `.js` file in `src/` is a listed section, and vice versa. */
function verifySections() {
	const found = readdirSync(src)
		.filter((name) => name.endsWith(".js") && name !== "build.js")
		.sort();
	const listed = [...SECTIONS].sort();
	if (found.join() !== listed.join()) {
		throw new Error(`src/ carries ${found.join(", ")}; SECTIONS lists ${listed.join(", ")}`);
	}
}

const freeze = process.argv.includes("--freeze");
const check = process.argv.includes("--check");

if (freeze) {
	const frozen = scan(readFileSync(0, "utf8"));
	const multiline = frozen.lines.find((line) => line.includes("\n"));
	if (multiline !== undefined) {
		throw new Error(`a canonical line carries a newline: ${multiline}`);
	}
	const header = [
		"# The pre-reorganization plugin (index.js), frozen for the build's own guard.",
		"# Comments stripped, blank lines dropped, the remaining lines collapsed and sorted.",
		"# Re-record with: git show <rev>:extensions/mnexis-opencode/index.js | node src/build.js --freeze",
	];
	const parts = [
		...header,
		...frozen.lines,
		MARKER_PAYLOADS,
		...frozen.payloads.map((text) => JSON.stringify(text)),
	];
	writeFileSync(baselinePath, `${parts.join("\n")}\n`);
	console.log(
		`froze ${frozen.lines.length} canonical lines and ${frozen.payloads.length} payloads into src/baseline.canonical.txt`,
	);
} else {
	verifySections();
	const generated = generate();
	const onDisk = readFileSync(indexPath, "utf8");
	const result = verify(generated);
	if (!check && onDisk !== generated) {
		writeFileSync(indexPath, generated);
	} else if (onDisk !== generated) {
		throw new Error("index.js is stale: run `node src/build.js`");
	}
	const glue = result.added === 0 ? "— identical modulo comments and order" : `(${result.added} glue lines added)`;
	console.log(
		[
			`index.js: ${result.lines} lines from ${result.sections} sections in src/`,
			`code lines: ${result.frozenLines} preserved, ${result.added} added ${glue}`,
			`payloads: ${result.payloads} quoted strings and templates preserved verbatim`,
		].join("\n"),
	);
}
