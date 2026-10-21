import { test } from "node:test";
import assert from "node:assert/strict";
import { normalizeRemoteUrl, workspaceKey, deriveNamespaceName, selectNamespace } from "../lib/workspace.js";

test("normalizeRemoteUrl strips https protocol, .git suffix and lowercases", () => {
	assert.equal(
		normalizeRemoteUrl("https://github.com/Xeworg/AegisPI.git"),
		"github.com-xeworg-aegispi",
	);
});

test("normalizeRemoteUrl handles scp-style ssh remotes", () => {
	assert.equal(
		normalizeRemoteUrl("git@github.com:Xeworg/AegisPI.git"),
		"github.com-xeworg-aegispi",
	);
});

test("normalizeRemoteUrl handles ssh:// protocol", () => {
	assert.equal(
		normalizeRemoteUrl("ssh://git@github.com/Xeworg/AegisPI.git"),
		"github.com-xeworg-aegispi",
	);
});

test("normalizeRemoteUrl keeps the host dots and replaces slashes and colons", () => {
	assert.equal(normalizeRemoteUrl("https://gitlab.com/group/sub/proj"), "gitlab.com-group-sub-proj");
});

test("normalizeRemoteUrl collapses repeated separators", () => {
	assert.equal(normalizeRemoteUrl("https://github.com//Xeworg//AegisPI"), "github.com-xeworg-aegispi");
});

test("normalizeRemoteUrl rejects empty input", () => {
	assert.throws(() => normalizeRemoteUrl(""), /empty/i);
	assert.throws(() => normalizeRemoteUrl(null), TypeError);
});

test("workspaceKey prefers the git remote", () => {
	assert.equal(
		workspaceKey({ remote: "https://github.com/Xeworg/AegisPI.git", repoRoot: "/w", cwd: "/w/sub" }),
		"git:github.com-xeworg-aegispi",
	);
});

test("workspaceKey falls back to the repo root when there is no remote", () => {
	assert.equal(workspaceKey({ remote: null, repoRoot: "/w", cwd: "/w/sub" }), "path:/w");
});

test("workspaceKey falls back to the cwd outside a repository", () => {
	assert.equal(workspaceKey({ remote: null, repoRoot: null, cwd: "/tmp/x" }), "path:/tmp/x");
});

test("deriveNamespaceName produces a short lowercase hyphenated name", () => {
	assert.equal(deriveNamespaceName("/home/xeworg/Proyectos/AegisPI"), "aegispi");
	assert.equal(deriveNamespaceName("/home/xeworg/Proyectos/My Project!"), "my-project");
	assert.equal(deriveNamespaceName("/"), "workspace");
});

test("selectNamespace keeps an existing binding and never renames it", () => {
	const bindings = [{ key: "git:github.com-xeworg-aegispi", namespace: "aegispi", path: "/w" }];
	const selection = selectNamespace({ bindings, key: "git:github.com-xeworg-aegispi", fallbackName: "other" });
	assert.deepEqual(selection, { namespace: "aegispi", bound: true });
});

test("selectNamespace derives a new namespace when the key is unknown", () => {
	const selection = selectNamespace({ bindings: [], key: "path:/tmp/x", fallbackName: "derived" });
	assert.deepEqual(selection, { namespace: "derived", bound: false });
});
