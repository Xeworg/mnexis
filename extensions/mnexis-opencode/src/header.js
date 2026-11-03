/**
 * mnexis for opencode — one file, both plugin models.
 *
 * opencode 1.x (`opencode-ai@1.19.5`) loads a module that exports a function
 * returning a hooks object; opencode 2.x (`@opencode/plugin@2.2.5`) loads a
 * module that default-exports `{ id, setup(ctx) }`. The two models never run
 * the same implementation ("V1 plugin implementations do not run in V2"), so
 * this file carries a default export both of them read:
 *
 *   default { id, setup, server }
 *
 *   - V1 detect-mode accepts any default object with an `id` and calls
 *     `.server(input, options)` (`packages/opencode/src/plugin/shared.ts`).
 *   - V2 decodes `{ id, setup }` and drops the extra member
 *     (`packages/core/src/plugin/module.ts`, an Effect `Schema.Struct`).
 *
 * `Plugin.define` is a runtime passthrough in V2, so nothing is imported from
 * opencode here: the file has no dependency and no `node_modules`.
 * `RESEARCH.md` records the sources and the one measured bet in this shape.
 *
 * What the plugin does — and nothing else:
 *
 * | rule        | why the host must carry it                          |
 * | ----------- | --------------------------------------------------- |
 * | capture     | `user_prompts` has no writer that does not need a model |
 * | session     | a session must land where its memories land          |
 * | pointer     | a session that never hears of the door never asks    |
 * | compaction  | the state must survive the context that held it      |
 * | settle      | the session's own outcomes show unsaved work         |
 * | gate        | permissions belong to the human, not to the agent    |
 * | definitions | the catalog the model reads is the host's (V2 only)  |
 *
 * The last rule is opencode 2 only: `ctx.tool.transform` is the one seat that
 * can rewrite an MCP tool's model-facing definition, and 1.x has no equivalent.
 * It adds no tool and changes no answer — only the words a model reads before
 * it decides to call one.
 *
 * The memory tools themselves are the MCP server's (`mnexis mcp serve`): one
 * shape for every host, never a second copy of the same facts. This file adds
 * only what MCP and the CLI cannot do — the parts the host owns.
 */

import { execFile as nodeExecFile } from "node:child_process";
