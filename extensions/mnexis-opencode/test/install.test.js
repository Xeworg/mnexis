/**
 * `install.sh`, against a throw-away config directory.
 *
 * The script is the install path while `mnexis agent install --env opencode`
 * cannot write an extension (RESEARCH.md §6), so its two promises are pinned
 * here: it installs exactly one file, and it removes only the file it wrote.
 */

import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const here = fileURLToPath(new URL("..", import.meta.url));
const script = join(here, "install.sh");
const source = join(here, "index.js");

/** Run the script with HOME and XDG pinned into the scratch directory. */
function run(args, { home }) {
	return new Promise((resolve) => {
		execFile(
			"bash",
			[script, ...args],
			{
				cwd: home,
				timeout: 60_000,
				env: { ...process.env, HOME: home, XDG_CONFIG_HOME: join(home, ".config") },
			},
			(error, stdout, stderr) => {
				resolve({ code: error && typeof error.code === "number" ? error.code : 0, stdout, stderr });
			},
		);
	});
}

async function scratch() {
	const home = await mkdtemp(join(tmpdir(), "mnexis-opencode-install-"));
	return {
		home,
		config: join(home, ".config", "opencode"),
		async done() {
			await rm(home, { recursive: true, force: true });
		},
	};
}

test("installing writes one file into the host's plugin directory", async () => {
	const box = await scratch();
	try {
		const result = await run([], { home: box.home });
		assert.equal(result.code, 0, result.stderr);
		const target = join(box.config, "plugins", "mnexis-memory.js");
		assert.ok(existsSync(target), "the plugin was installed");
		assert.equal(await readFile(target, "utf8"), await readFile(source, "utf8"));
		// The other half is the engine's, and the script says so.
		assert.match(result.stdout, /mnexis agent install --env opencode/);
	} finally {
		await box.done();
	}
});

test("installing twice is the same installation", async () => {
	const box = await scratch();
	try {
		await run([], { home: box.home });
		const target = join(box.config, "plugins", "mnexis-memory.js");
		const first = await readFile(target, "utf8");
		const second = await run([], { home: box.home });
		assert.equal(second.code, 0, second.stderr);
		assert.match(second.stdout, /already installed/);
		assert.equal(await readFile(target, "utf8"), first);
	} finally {
		await box.done();
	}
});

test("uninstalling removes only the file it wrote", async () => {
	const box = await scratch();
	try {
		await run([], { home: box.home });
		const plugins = join(box.config, "plugins");
		const target = join(plugins, "mnexis-memory.js");
		// A neighbour the plugin does not own survives every path.
		const neighbour = join(plugins, "someone-else.js");
		await writeFile(neighbour, "export default { id: 'other' }\n", "utf8");

		const removed = await run(["--uninstall"], { home: box.home });
		assert.equal(removed.code, 0, removed.stderr);
		assert.match(removed.stdout, /removed/);
		assert.equal(existsSync(target), false);
		assert.equal(existsSync(neighbour), true);

		const again = await run(["--uninstall"], { home: box.home });
		assert.equal(again.code, 0, again.stderr);
		assert.match(again.stdout, /nothing to remove/);
	} finally {
		await box.done();
	}
});

test("uninstalling refuses a file that is no longer ours", async () => {
	const box = await scratch();
	try {
		const plugins = join(box.config, "plugins");
		await mkdir(plugins, { recursive: true });
		const target = join(plugins, "mnexis-memory.js");
		const edited = "// the human changed this file\nexport default { id: 'mnexis' }\n";
		await writeFile(target, edited, "utf8");

		const refused = await run(["--uninstall"], { home: box.home });
		assert.notEqual(refused.code, 0);
		assert.match(refused.stderr, /differs from the plugin/);
		assert.equal(await readFile(target, "utf8"), edited);

		const forced = await run(["--uninstall", "--force"], { home: box.home });
		assert.equal(forced.code, 0, forced.stderr);
		assert.equal(existsSync(target), false);
	} finally {
		await box.done();
	}
});

test("the configuration directory can be named explicitly", async () => {
	const box = await scratch();
	try {
		const elsewhere = join(box.home, "somewhere", "opencode");
		const result = await run(["--config-dir", elsewhere], { home: box.home });
		assert.equal(result.code, 0, result.stderr);
		assert.ok(existsSync(join(elsewhere, "plugins", "mnexis-memory.js")));
		// The default location was not touched.
		assert.equal(existsSync(join(box.config, "plugins")), false);
	} finally {
		await box.done();
	}
});

test("an unknown argument is refused instead of guessed at", async () => {
	const box = await scratch();
	try {
		const result = await run(["--instal"], { home: box.home });
		assert.equal(result.code, 2);
		assert.match(result.stderr, /unknown argument/);
		assert.equal(existsSync(join(box.config, "plugins")), false);
	} finally {
		await box.done();
	}
});
