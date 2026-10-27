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
 *
 * A marker names, it never grants: the namespace it returns here with
 * `bound: false` is a candidate the human is asked about, and only the
 * human's confirmation binds it (auto-namespace S6).
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

/** The reserved namespace no workspace may be bound to (S2, S4). */
const RESERVED_GLOBAL = "global";

/** Why a workspace with no usable name offers no candidate. */
export const NO_NAME_REFUSAL =
	"this workspace has no name to offer: name its namespace yourself with " +
	"`mnexis project bind <namespace> --key <key>`";

/** Why the reserved namespace is never offered as a candidate. */
export const RESERVED_GLOBAL_REFUSAL =
	"`global` is reserved and a workspace cannot be bound to it: pick another " +
	"namespace with `mnexis project bind <namespace> --key <key>`";

/**
 * The candidate namespace the engine published for this workspace, read
 * from `mnexis status --json` (auto-namespace S2).
 *
 * `answered` tells "the engine says there is no candidate" (outside a
 * repository, which is not a project) apart from "the engine could not be
 * asked"; only the second case leaves the extension to derive the name
 * itself.
 *
 * @param {unknown} statusData
 * @returns {{ answered: boolean, namespace: string | null, refusal: string | null }}
 */
export function readWorkspaceCandidate(statusData) {
	const workspace =
		statusData && typeof statusData === "object" ? statusData.workspace : null;
	if (!workspace || typeof workspace !== "object") {
		return { answered: false, namespace: null, refusal: null };
	}
	// Only the reserved name and an empty name are vetted here: whatever
	// else the engine derived is the engine's answer. `global` never reaches
	// the human through this path, even when an engine publishes it as a
	// candidate instead of refusing it (auto-namespace S4).
	const published =
		typeof workspace.candidate_namespace === "string" && workspace.candidate_namespace !== ""
			? candidateName(workspace.candidate_namespace)
			: { namespace: null, refusal: null };
	const refusal =
		typeof workspace.candidate_refusal === "string" && workspace.candidate_refusal !== ""
			? workspace.candidate_refusal
			: published.refusal;
	return { answered: true, namespace: refusal ? null : published.namespace, refusal };
}

/**
 * The candidate the marker names, which is still only a candidate (S6).
 *
 * @param {string | null} marker
 * @returns {{ namespace: string | null, refusal: string | null }}
 */
export function candidateFromMarker(marker) {
	return candidateName(marker);
}

/**
 * Derive the candidate namespace with the S2 rules, for the one case the
 * engine cannot answer: the last segment of the normalized remote,
 * lowercased, or the repository directory's own name when there is no
 * remote.
 *
 * This is the extension's copy of the engine's rule, never a second source
 * of truth: `resolveWorkspace` reads the engine's answer first and falls
 * back here only when `mnexis status` cannot be asked.
 *
 * @param {{ remote?: string | null, repoRoot?: string | null }} input
 * @returns {{ namespace: string | null, refusal: string | null }}
 */
export function deriveWorkspaceCandidate({ remote, repoRoot } = {}) {
	const tail = remoteTail(remote);
	if (tail !== null) {
		return candidateName(tail);
	}
	if (!repoRoot) {
		// Outside a repository there is no project to name.
		return { namespace: null, refusal: null };
	}
	// The engine lowercases the repository's own name the same way.
	return candidateName(path.basename(String(repoRoot)).toLowerCase());
}

/** The last path segment of a remote, without `.git` and lowercased. */
function remoteTail(remote) {
	if (typeof remote !== "string") {
		return null;
	}
	const trimmed = remote.trim().replace(/\/+$/, "");
	if (trimmed === "") {
		return null;
	}
	const parts = trimmed.split(/[/:]/).filter((part) => part !== "");
	const last = parts.length > 0 ? parts[parts.length - 1] : "";
	const tail = last.replace(/\.git$/i, "").trim().toLowerCase();
	return tail === "" || tail === "." || tail === ".." ? null : tail;
}

/** Normalize and vet one candidate name: the engine's refusal rules. */
function candidateName(name) {
	const trimmed = typeof name === "string" ? name.trim() : "";
	if (trimmed === "") {
		return { namespace: null, refusal: NO_NAME_REFUSAL };
	}
	if (trimmed === RESERVED_GLOBAL) {
		return { namespace: null, refusal: RESERVED_GLOBAL_REFUSAL };
	}
	return { namespace: trimmed, refusal: null };
}
