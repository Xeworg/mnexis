// --- pure decisions (every one of them is pinned by a test) ----------------

/**
 * The commands and file writes that would grant write permissions without
 * the human. The plugin owns that rule on every host, so a model-issued call
 * that tries to do it behind the plugin's back is refused.
 *
 * The rule is this host's own copy of the invariant: a grant means the same
 * thing on every host, and two copies that disagree would be a hole in one of
 * them. The engine's own command names are the contract.
 */
const PERMISSION_GRANT_PATTERN =
	/\bmnexis\s+config\s+allow-namespace\b|\bmnexis\s+project\s+bind\b/;

/** The profile files a model must never edit by hand. */
const PROFILE_FILE_PATTERN = /(?:^|[\\/])(mnexis|projects)\.toml$/;

/** The shell tools: V1 calls it `bash`, V2 calls it `shell`. */
const SHELL_TOOLS = new Set(["bash", "shell"]);

/** The file-writing tools: V2 adds `patch`. */
const EDIT_TOOLS = new Set(["edit", "write", "patch"]);

/** What a blocked grant says. opencode cannot ask the human from a plugin. */
export const GRANT_REFUSAL =
	"Granting a namespace is the human's decision, and this host cannot ask them from " +
	"inside a plugin: nothing was granted. Ask the human to run " +
	"`mnexis config allow-namespace <namespace>` once — the grant persists in the " +
	"profile and the retry works — or `mnexis project bind <namespace> --key <key>` " +
	"for a workspace with no namespace yet.";

/** What a blocked profile edit says. */
export const PROFILE_REFUSAL =
	"The mnexis profile files are managed by the mnexis CLI, never edited by hand. " +
	"Use `mnexis init`, `mnexis project bind` or `mnexis config allow-namespace` instead.";

/**
 * The refusal a tool call earns, or `null` when it is the model's to make.
 *
 * @param {string} tool the host's tool id
 * @param {unknown} input the arguments the model passed
 * @returns {string | null}
 */
export function gateViolation(tool, input) {
	const name = typeof tool === "string" ? tool : "";
	if (SHELL_TOOLS.has(name)) {
		const command = typeof input?.command === "string" ? input.command : "";
		return PERMISSION_GRANT_PATTERN.test(command) ? GRANT_REFUSAL : null;
	}
	if (EDIT_TOOLS.has(name)) {
		// V1 and V2 name the target `filePath`; a tool that names it `path`
		// is checked too rather than trusted.
		const target =
			typeof input?.filePath === "string"
				? input.filePath
				: typeof input?.path === "string"
					? input.path
					: "";
		return PROFILE_FILE_PATTERN.test(target) ? PROFILE_REFUSAL : null;
	}
	return null;
}
