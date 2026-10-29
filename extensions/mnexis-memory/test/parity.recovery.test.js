/**
 * Parity: the same repair against the real engine (odd/tasks/store-recovery.md,
 * S1-S3, L4).
 *
 * The corpus in `recovery.test.js` runs against a fake that mirrors the engine
 * (oldest-first `list`, no `memory_history` row on save or supersede, the
 * engine's supersede failures). A fake can drift from the engine silently —
 * that is exactly how the first implementation passed its tests and failed in
 * production (L4/B1, L4/B4) — so this file asks the engine itself.
 *
 * It runs only when `MNEXIS_PARITY_BIN` names a real `mnexis` binary:
 *
 *   MNEXIS_PARITY_BIN=~/.local/bin/mnexis node --test \
 *     "extensions/mnexis-memory/test/parity.recovery.test.js"
 *
 * It is hermetic by construction: every run creates its own data directory and
 * its own workspace directory under the system temp directory, points
 * `MNEXIS_DATA_DIR` at the former, and asserts the engine agrees before it
 * writes anything. The user's profile is never opened.
 */

import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import mnexisMemory from "../index.js";
import { createClient } from "../lib/cli.js";

const NAMESPACE = "payments";
const PARITY_BINARY = process.env.MNEXIS_PARITY_BIN ?? "";
const skip = PARITY_BINARY === "" ? "set MNEXIS_PARITY_BIN to a mnexis binary to run the parity checks" : false;

/** A real engine in a data directory nobody else can reach. */
async function realEngine(t) {
	const dataDir = mkdtempSync(join(tmpdir(), "mnexis-parity-data-"));
	const workDir = mkdtempSync(join(tmpdir(), "mnexis-parity-work-"));
	t.after(() => {
		rmSync(dataDir, { recursive: true, force: true });
		rmSync(workDir, { recursive: true, force: true });
	});
	process.env.MNEXIS_BIN = PARITY_BINARY;
	process.env.MNEXIS_DATA_DIR = dataDir;
	const client = createClient({ binary: PARITY_BINARY, env: { ...process.env, MNEXIS_DATA_DIR: dataDir } });

	const init = await client.init(workDir);
	assert.equal(init.code, 0, "the profile is initialised in the temp data dir");
	const config = await client.configShow(workDir);
	assert.equal(
		config.data?.data_dir,
		dataDir,
		"the parity run is isolated: every write lands in the temp data dir",
	);

	// Bind this temp workspace to the namespace, the way a human would. Both
	// permissions go to the temp profile, never to the user's.
	const listed = await client.projectList(workDir);
	const key = listed.data?.current_key;
	assert.ok(key, "the engine publishes the workspace key it resolves");
	const bind = await client.bind(NAMESPACE, key, workDir, workDir);
	assert.equal(bind.code, 0, `bind failed: ${bind.stderr.trim()}`);
	const allow = await client.allowNamespace(NAMESPACE, workDir);
	assert.equal(allow.code, 0, `allow-namespace failed: ${allow.stderr.trim()}`);

	const tools = new Map();
	const entries = [];
	mnexisMemory({
		registerTool: (tool) => tools.set(tool.name, tool),
		on() {},
		appendEntry: (type, data) => entries.push({ type, data }),
	});
	return { client, workDir, tools, entries };
}

/** The pi context of a headless tool call in this workspace. */
function context(workDir) {
	return {
		cwd: workDir,
		hasUI: false,
		mode: "print",
		ui: { confirm: async () => false },
	};
}

/** Every active record of the namespace, read page by page from the engine. */
async function activeRecords(client, workDir) {
	const items = [];
	for (let offset = 0; ; offset += 100) {
		const page = await client.run(
			[
				"list",
				"--namespace",
				NAMESPACE,
				"--state",
				"active",
				"--limit",
				"100",
				"--offset",
				String(offset),
				"--full",
			],
			{ json: true, cwd: workDir },
		);
		assert.equal(page.code, 0, `list failed: ${page.stderr.trim()}`);
		const rows = page.data?.items ?? [];
		items.push(...rows);
		if (rows.length < 100) {
			return items;
		}
	}
}

/** The active records that carry the value AND the fact's vocabulary. */
function liveForFact(items, value, keyword) {
	return items.filter(
		(item) =>
			new RegExp(`(?:^|[^0-9])${value}(?:[^0-9]|$)`).test(item.content ?? "") &&
			(item.keywords ?? []).some((word) => word.toLowerCase() === keyword),
	);
}

/** Save `count` records that give the namespace size but share nothing. */
async function fillers(client, workDir, count, filler) {
	for (let index = 0; index < count; index += 1) {
		const saved = await client.save(filler(index), workDir);
		assert.equal(saved.code, 0, `filler ${index} failed: ${saved.stderr.trim()}`);
	}
}

test("parity: a twin past the page the engine returns is repaired (B1)", { skip }, async (t) => {
	const { client, workDir, tools, entries } = await realEngine(t);
	await fillers(client, workDir, 140, (index) => ({
		namespace: NAMESPACE,
		title: `Nota de trabajo ${index}`,
		content: `Nota de trabajo sobre el despliegue y su operacion diaria ${index}.`,
		keywords: "nota, bitacora",
	}));
	const older = await client.save(
		{
			namespace: NAMESPACE,
			title: "Puerto de escucha del servicio",
			content: "El servicio payments-api escucha en el puerto 8081 (4 workers).",
			keywords: "puerto, port",
		},
		workDir,
	);
	assert.equal(older.code, 0, `the twin failed to save: ${older.stderr.trim()}`);

	// The real `mnexis list` answers the OLDEST rows, so the record this save
	// writes is not in any first page of the namespace: a repair that looks
	// for it there does nothing at all (L4/B1).
	const saved = await tools.get("mnexis_save").execute(
		"call-1",
		{
			title: "Puerto de la API de pagos",
			content: "El puerto del servicio es 8081.",
			keywords: "puerto, listen port",
		},
		undefined,
		undefined,
		context(workDir),
	);
	assert.notEqual(saved.isError, true, "the save succeeded");
	assert.equal(entries.length, 1, "the intervention is journaled");
	assert.equal(entries[0].data.older.uuid, older.data.uuid, "the twin was retired");

	const live = liveForFact(await activeRecords(client, workDir), "8081", "puerto");
	assert.deepEqual(
		live.map((item) => item.uuid),
		[saved.details.uuid],
		"the fact ends with exactly one live record: the one the save wrote",
	);

	// S3, amended: the row is kept and visible as `superseded`.
	const shown = await client.show(older.data.uuid, workDir);
	assert.equal(shown.code, 0);
	assert.equal(shown.data?.state, "superseded", "show reports the row as superseded");
	assert.equal(shown.data?.content, "El servicio payments-api escucha en el puerto 8081 (4 workers).");

	// S3, amended: no `memory_history` row was written, exactly as the engine
	// behaves — the fiction L4/B4 refused.
	const history = await client.history({ uuid: older.data.uuid }, workDir);
	assert.equal(history.code, 0);
	assert.equal(history.data?.count, 0, "the engine wrote no history row on supersede");

	const timeline = await client.run(["timeline", older.data.uuid], { json: true, cwd: workDir });
	assert.equal(timeline.code, 0);
	assert.equal(timeline.data?.superseded_by?.[0]?.uuid, saved.details.uuid);
});

test("parity: a twin behind a saturated ranked answer is repaired (B1)", { skip }, async (t) => {
	const { client, workDir, tools } = await realEngine(t);
	// 120 one-line records that all carry the value: they fill the engine's
	// ranked page (`MAX_LIMIT`, 100) without ever being twins of this fact.
	await fillers(client, workDir, 120, (index) => ({
		namespace: NAMESPACE,
		title: `Ticket ${index}`,
		content: "8081",
		keywords: "ticket",
	}));
	const older = await client.save(
		{
			namespace: NAMESPACE,
			title: "Puerto de escucha del servicio",
			content:
				"El puerto del servicio de pagos que atiende a los clientes externos y a los internos es 8081.",
			keywords: "puerto, port",
		},
		workDir,
	);
	assert.equal(older.code, 0, `the twin failed to save: ${older.stderr.trim()}`);

	const saved = await tools.get("mnexis_save").execute(
		"call-1",
		{
			title: "Puerto de la API de pagos",
			content: "El puerto del servicio es 8081.",
			keywords: "puerto, listen port",
		},
		undefined,
		undefined,
		context(workDir),
	);
	assert.notEqual(saved.isError, true, "the save succeeded");
	assert.equal(saved.details.recovery?.source, "list", "the saturated answer sent the repair to the whole namespace");
	assert.equal(saved.details.recovery?.superseded?.[0]?.uuid, older.data.uuid);

	const live = liveForFact(await activeRecords(client, workDir), "8081", "puerto");
	assert.deepEqual(live.map((item) => item.uuid), [saved.details.uuid], "exactly one live record for the fact");
	assert.equal(
		(await activeRecords(client, workDir)).filter((item) => item.title.startsWith("Ticket ")).length,
		120,
		"the records that only share the value were never touched",
	);
});
