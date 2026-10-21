import { test } from "node:test";
import assert from "node:assert/strict";
import { isPermissionGrantCommand, isProfileFilePath, needsProfileInit, readItems } from "../index.js";

test("readItems unwraps the { count, items } envelope", () => {
	assert.deepEqual(readItems({ count: 1, items: [{ uuid: "a" }] }), [{ uuid: "a" }]);
});

test("readItems accepts a bare array and tolerates anything else", () => {
	assert.deepEqual(readItems([{ uuid: "a" }]), [{ uuid: "a" }]);
	assert.deepEqual(readItems(null), []);
	assert.deepEqual(readItems({ count: 0 }), []);
	assert.deepEqual(readItems("nonsense"), []);
});

test("a command that grants a namespace is recognized", () => {
	assert.equal(isPermissionGrantCommand("mnexis config allow-namespace aegispi"), true);
	assert.equal(isPermissionGrantCommand("cd /w && mnexis config allow-namespace aegispi 2>&1"), true);
	assert.equal(isPermissionGrantCommand("mnexis project bind foo --key k --path /w"), true);
});

test("ordinary mnexis commands are not blocked", () => {
	assert.equal(isPermissionGrantCommand("mnexis save --namespace aegispi 'text'"), false);
	assert.equal(isPermissionGrantCommand("mnexis search 'query'"), false);
	assert.equal(isPermissionGrantCommand("mnexis config show"), false);
	assert.equal(isPermissionGrantCommand("ls -la"), false);
});

test("the profile files are recognized by name", () => {
	assert.equal(isProfileFilePath("/root/.local/share/mnexis/mnexis.toml"), true);
	assert.equal(isProfileFilePath("/root/.local/share/mnexis/projects.toml"), true);
	assert.equal(isProfileFilePath("/w/README.md"), false);
	assert.equal(isProfileFilePath(undefined), false);
});

test("a missing profile settings file must be initialized", () => {
	assert.equal(needsProfileInit({ config_file_present: false }), true);
	assert.equal(needsProfileInit({ config_file_present: true }), false);
	assert.equal(needsProfileInit(null), true);
});
