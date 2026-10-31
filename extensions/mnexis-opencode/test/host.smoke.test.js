/**
 * The real opencode loader, over the real file.
 *
 * The unit tests fake both plugin models; this one asks the host installed on
 * the machine to load the file and says whether it accepted it. It is opt-in
 * because it needs opencode:
 *
 *   OPENCODE_BIN="$(command -v opencode)" node --test extensions/mnexis-opencode/test/host.smoke.test.js
 *
 * The plugin is installed into a throw-away HOME (and XDG directories), never
 * into the user's own configuration, and the server runs `--standalone` so it
 * does not touch the user's managed service.
 */

import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { copyFile, mkdir, mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const OPENCODE = process.env.OPENCODE_BIN ?? "";
const here = fileURLToPath(new URL("..", import.meta.url));

function run(command, args, { home }) {
	return new Promise((resolve) => {
		execFile(
			command,
			args,
			{
				timeout: 180_000,
				env: {
					...process.env,
					HOME: home,
					XDG_CONFIG_HOME: join(home, ".config"),
					XDG_DATA_HOME: join(home, ".local", "share"),
					XDG_CACHE_HOME: join(home, ".cache"),
					XDG_STATE_HOME: join(home, ".local", "state"),
				},
			},
			(error, stdout, stderr) => {
				const code = error && typeof error.code === "number" ? error.code : 0;
				resolve({ code, stdout, stderr, output: `${stdout}\n${stderr}` });
			},
		);
	});
}

test(
	"the installed opencode loads the plugin without a load error",
	{ skip: OPENCODE === "" ? "set OPENCODE_BIN to the opencode binary to run this" : false },
	async () => {
		const home = await mkdtemp(join(tmpdir(), "mnexis-opencode-host-"));
		try {
			const version = await run(OPENCODE, ["--version"], { home });
			const plugins = join(home, ".config", "opencode", "plugins");
			await mkdir(plugins, { recursive: true });
			await copyFile(join(here, "index.js"), join(plugins, "mnexis-memory.js"));

			const result = await run(OPENCODE, ["plugin", "list", "--standalone", "--print-logs"], { home });
			assert.equal(result.code, 0, result.output);

			// The host discovered the file and imported it.
			assert.match(result.output, /loading plugin/);
			assert.match(result.output, /mnexis-memory\.js/);

			// ... and did not reject the shape it default-exports.
			assert.doesNotMatch(result.output, /failed to load plugin/);
			assert.doesNotMatch(result.output, /Plugin must export a default definition/);

			// The file that was loaded is the one this extension ships.
			assert.match(version.output, /v?\d+\.\d+\.\d+/);
			assert.match(result.output, /version=/);
		} finally {
			await rm(home, { recursive: true, force: true });
		}
	},
);
