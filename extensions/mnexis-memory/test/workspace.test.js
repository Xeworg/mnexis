import { test } from "node:test";
import assert from "node:assert/strict";
import {
	deriveWorkspaceCandidate,
	findMarkerNamespace,
	normalizeRemoteUrl,
	selectNamespace,
	workspaceKey,
} from "../lib/workspace.js";

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



test("selectNamespace keeps an existing binding and never renames it", () => {
	const bindings = [{ key: "git:github.com-xeworg-aegispi", namespace: "aegispi", path: "/w" }];
	const selection = selectNamespace({ bindings, key: "git:github.com-xeworg-aegispi", fallbackName: "other" });
	assert.deepEqual(selection, { namespace: "aegispi", bound: true });
});

test("selectNamespace derives a new namespace when the key is unknown", () => {
	const selection = selectNamespace({ bindings: [], key: "path:/tmp/x", fallbackName: "derived" });
	assert.deepEqual(selection, { namespace: "derived", bound: false });
});

test("findMarkerNamespace reads the nearest marker at or above the directory", () => {
	const files = {
		"/w/project/.mnexis-namespace": "ns-marcado\n",
	};
	const readFile = (file) => {
		if (!(file in files)) {
			throw new Error("ENOENT");
		}
		return files[file];
	};
	assert.equal(findMarkerNamespace("/w/project", { readFile }), "ns-marcado");
	assert.equal(findMarkerNamespace("/w/project/sub", { readFile }), "ns-marcado");
});

test("findMarkerNamespace keeps walking when the marker is empty", () => {
	const files = {
		"/w/project/.mnexis-namespace": "   \n",
		"/w/.mnexis-namespace": "ns-de-arriba\n",
	};
	const readFile = (file) => {
		if (!(file in files)) {
			throw new Error("ENOENT");
		}
		return files[file];
	};
	assert.equal(findMarkerNamespace("/w/project/sub", { readFile }), "ns-de-arriba");
});

test("findMarkerNamespace answers null when there is no marker", () => {
	const readFile = () => {
		throw new Error("ENOENT");
	};
	assert.equal(findMarkerNamespace("/w/project", { readFile }), null);
});

// --- the candidate namespace, the one fallback the engine cannot answer ---
//
// `resolveWorkspace` reads the candidate from `mnexis status --json`; this
// derivation runs only when the engine cannot be asked, and it has to follow
// the same rules so the two answers never disagree.

test("deriveWorkspaceCandidate takes the last segment of the remote", () => {
	assert.deepEqual(deriveWorkspaceCandidate({ remote: "https://github.com/Xeworg/Nodexar.git" }), {
		namespace: "nodexar",
		refusal: null,
	});
	// A dashed repository name keeps its dashes: the name is the remote's
	// last segment, not a piece of the normalized key.
	assert.equal(
		deriveWorkspaceCandidate({ remote: "git@github.com:Xeworg/my-project.git" }).namespace,
		"my-project",
	);
});

test("deriveWorkspaceCandidate names the folder when there is no remote", () => {
	// A repository without a remote is named by its root.
	assert.equal(
		deriveWorkspaceCandidate({ remote: null, repoRoot: "/w/MyProject" }).namespace,
		"myproject",
	);
	// A directory that is not a repository is named by its own folder: a new
	// project before `git init` still has a name.
	assert.equal(
		deriveWorkspaceCandidate({ remote: null, cwd: "/w/Nuevo-Proyecto" }).namespace,
		"nuevo-proyecto",
	);
	// The repository root wins over the working directory inside it.
	assert.equal(
		deriveWorkspaceCandidate({ remote: null, repoRoot: "/w/Proyecto", cwd: "/w/Proyecto/sub" })
			.namespace,
		"proyecto",
	);
});

test("deriveWorkspaceCandidate refuses the reserved name and offers nothing with no folder", () => {
	assert.match(
		deriveWorkspaceCandidate({ remote: null, cwd: "/w/GLOBAL" }).refusal,
		/global/,
		"`global` is reserved and never offered",
	);
	assert.deepEqual(deriveWorkspaceCandidate({}), { namespace: null, refusal: null });
});
