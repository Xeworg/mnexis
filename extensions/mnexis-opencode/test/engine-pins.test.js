/**
 * The engine pins: the copy the plugin keeps is the engine's.
 *
 * The curated definitions and the seven glosses are an inline copy in the one
 * file the plugin must stay, because the plugin cannot import from the engine.
 * A copy that may drift is a copy nobody can trust, so each pin reads the
 * engine's own source, from the repository root. The engine is the only contract
 * the hosts share; no pin reads another host's extension tree.
 */

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import { CURATED_DESCRIPTIONS, TYPE_GLOSSES } from "../index.js";

// --- the engine pins (odd/tasks/opencode-parity.md, S1+S2) -----------------
//
// The curated definitions and the seven glosses are an inline copy in the one
// flat file the plugin must stay, because the plugin cannot import from the
// engine. A copy that may drift is a copy nobody can trust, so each pin reads
// the engine's own source, from the repository root. The engine is the only
// contract the hosts share; no pin reads another host's extension tree.

const REPO_ROOT = fileURLToPath(new URL("../../..", import.meta.url));

/** A repository file, read from disk — the copy is pinned to the file. */
function repoFile(relative) {
	return readFileSync(join(REPO_ROOT, relative), "utf8");
}

/**
 * The engine's seven type names, read from its own source.
 *
 * `SaveArgs::memory_type`'s doc comment states the closed set ("Memory type:
 * knowledge, decision, preference, procedure, project_state, episode or
 * fact."), and schemars carries that doc into the MCP input schema every host
 * reads. The extraction drops the `///` markers, so a reflowed comment is
 * still the same list.
 */
function engineTypeNames() {
	const source = repoFile("src/mcp/tools.rs");
	const match = source.match(/Memory type:\s*([^.]*)\./);
	assert.ok(match, "the engine's save argument states the memory types");
	const text = match[1]
		.split("\n")
		.map((line) => line.replace(/^\s*\/\/+\s*/, "").trim())
		.join(" ");
	return text
		.split(/\s*(?:,|\bor\b)\s*/)
		.map((name) => name.trim())
		.filter((name) => name !== "");
}

test("the plugin's seven type names are the engine's closed set, and the gloss is the plugin's own", () => {
	const engine = engineTypeNames();
	assert.equal(engine.length, 7, `the engine states seven kinds: ${engine.join(", ")}`);
	assert.equal(new Set(engine).size, 7, "the engine's names are distinct");

	const plugin = TYPE_GLOSSES.map((gloss) => {
		const [name, ...rest] = String(gloss).split(" — ");
		assert.notEqual(rest.join(" — ").trim(), "", `a gloss is a name and one line: ${gloss}`);
		assert.doesNotMatch(rest.join(" — "), /\n/, `a gloss is one line: ${gloss}`);
		return name;
	});

	assert.deepEqual(
		plugin,
		engine,
		"the plugin's names are the engine's, in the engine's order: the engine is the shared contract",
	);
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
