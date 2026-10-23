/**
 * Workspace → namespace resolution.
 *
 * The workspace key must be byte-identical across agents and sessions, so
 * the normalization is fully specified here: drop the protocol, drop the
 * `.git` suffix, lowercase the whole remote URL, and replace `/` and `:`
 * with `-` while keeping the host's dots.
 *
 *   https://github.com/Xeworg/AegisPI.git  ->  git:github.com-xeworg-aegispi
 *   git@github.com:Xeworg/AegisPI.git       ->  git:github.com-xeworg-aegispi
 */

import path from "node:path";
import { readFileSync } from "node:fs";

/** Normalize a git remote URL into the canonical key suffix. */
export function normalizeRemoteUrl(remote) {
	if (typeof remote !== "string") {
		throw new TypeError("remote must be a string");
	}
	let url = remote.trim();
	if (url === "") {
		throw new Error("remote URL is empty");
	}
	url = url.replace(/^git\+/i, "");
	url = url.replace(/^[a-z][a-z0-9+.-]*:\/\//i, "");
	url = url.replace(/^[^/@]+@/, "");
	url = url.replace(/\.git$/i, "");
	url = url.toLowerCase();
	url = url.replace(/[/:]+/g, "-");
	url = url.replace(/-+/g, "-").replace(/^-+|-+$/g, "");
	if (url === "") {
		throw new Error(`remote URL '${remote}' has no usable path`);
	}
	return url;
}

/**
 * Build the workspace key.
 *
 * @param {{ remote?: string | null, repoRoot?: string | null, cwd: string }} input
 */
export function workspaceKey({ remote, repoRoot, cwd }) {
	if (remote) {
		return `git:${normalizeRemoteUrl(remote)}`;
	}
	if (repoRoot) {
		return `path:${repoRoot}`;
	}
	return `path:${cwd}`;
}

/**
 * Decide which namespace to use for a workspace key.
 *
 * An existing binding is never renamed: its namespace wins.
 *
 * The fallback is only the marker: a namespace the human wrote down. The
 * name of a directory is never a namespace — inventing one leaves the
 * workspace bound to a namespace the allowlist does not know, and the
 * write is refused with a dead end the agent cannot get out of.
 */
export function selectNamespace({ bindings, key, fallbackName }) {
	const list = Array.isArray(bindings) ? bindings : [];
	const existing = list.find((binding) => binding && binding.key === key);
	if (existing && typeof existing.namespace === "string" && existing.namespace !== "") {
		return { namespace: existing.namespace, bound: true };
	}
	return {
		namespace: typeof fallbackName === "string" && fallbackName !== "" ? fallbackName : null,
		bound: false,
	};
}

/** The marker file a workspace can drop to name its namespace. */
export const MARKER_FILE = ".mnexis-namespace";

/**
 * The namespace named by the nearest `.mnexis-namespace` marker at or above
 * `start`.
 *
 * The marker holds one namespace name, trimmed. A marker with no name names
 * nothing and the walk continues upwards, so an accidentally empty file
 * cannot silently unbind a nested project. The mnexis CLI resolves the
 * namespace the same way, so both sides agree on which project a directory
 * belongs to.
 *
 * @param {string} startPath
 * @param {{ readFile?: Function }} options
 * @returns {string | null}
 */
export function findMarkerNamespace(startPath, options = {}) {
	const readFile = options.readFile ?? defaultReadFile;
	let current = startPath;
	for (;;) {
		const parent = path.dirname(current);
		try {
			const name = String(readFile(path.join(current, MARKER_FILE)) ?? "").trim();
			if (name !== "") {
				return name;
			}
		} catch {
			// No marker here: keep walking up.
		}
		if (parent === current) {
			return null;
		}
		current = parent;
	}
}

/** Read a file as UTF-8. Injected in the tests. */
function defaultReadFile(file) {
	return readFileSync(file, "utf8");
}
