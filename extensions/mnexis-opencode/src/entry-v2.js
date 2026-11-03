// --- opencode 2.x (odd/tasks/opencode-parity.md, S3) -----------------------
//
// Moved out of `createPlugin` by the reorganization: the assembly composes the
// two entry points, and each model's adapter lives with the model it serves.

/**
 * The journal this host has.
 *
 * V1 hands a plugin `client.app.log`; V2's plugin context has no logger at all
 * (RESEARCH §1), and its `ctx.storage` is a key/value store — `StorageDomain`:
 * `get`/`set`/`remove`/`scan` (`@opencode/plugin`'s `storage.d.ts`). A report
 * written there is one key per kind, so a refusal is observable instead of
 * swallowed — which is what the audit found: `report()` had no sink on V2.
 *
 * @param {unknown} storage the host's `ctx.storage`, or nothing
 * @returns {((record: object) => void) | undefined}
 */
export function storageLogger(storage) {
	if (!storage || typeof storage.set !== "function") {
		return undefined;
	}
	return (record) => {
		const kind = typeof record?.kind === "string" && record.kind !== "" ? record.kind : "report";
		try {
			void Promise.resolve(
				storage.set(`mnexis.report.${kind}`, { at: new Date().toISOString(), ...record }),
			).catch(() => {
				// The journal is a report, never a precondition.
			});
		} catch {
			// Same: a store that refuses the report never fails the turn.
		}
	};
}

/**
 * The V2 entry: the hooks this model's domains own, registered on them.
 *
 * `coreFor(cwd, logger)` builds one core for the instance, and everything else
 * here is the host's shape.
 */
function createV2Entry(coreFor) {
	async function setup(ctx = {}) {
		const cwd = typeof ctx.location?.directory === "string" ? ctx.location.directory : process.cwd();
		// V2 has no logger: the host's storage is where a report survives.
		const core = coreFor(cwd, storageLogger(ctx.storage));
		const controllers = [];

		// Each seat is registered on its own: a host that does not have one
		// (an older 2.x without `compaction`, a test double, a future rename)
		// must still get the rest, rather than lose the whole plugin — the
		// capture, above all, is the rule nothing else provides.
		if (ctx.session && typeof ctx.session.hook === "function") {
			// The prompt hook runs once, at admission, for what a person
			// submitted — the host already excludes synthetic messages, shell
			// messages and compaction controls, so no filter is needed here.
			try {
				await ctx.session.hook("prompt", async (event) => {
					await core.capture(event?.sessionID, event?.prompt?.text);
				});
			} catch {
				// No prompt seat: the other rules still run.
			}

			// The system prompt is assembled per model request. The pointer is
			// delivered once per session and is a no-op afterwards; the settle
			// question rides the same seat, once, when the session's own
			// outcomes show unsaved work.
			try {
				await ctx.session.hook("context", async (event) => {
					if (!Array.isArray(event?.system)) {
						return;
					}
					const pointer = await core.pointer(event?.sessionID);
					if (pointer) {
						event.system.push({ type: "text", text: pointer });
					}
					const settle = core.settle(event?.sessionID);
					if (settle) {
						event.system.push({ type: "text", text: settle });
					}
				});
			} catch {
				// No context seat: the pointer simply is not delivered.
			}

			// Compaction keeps the live state the transcript was carrying.
			try {
				await ctx.session.hook("compaction", async (event) => {
					const block = await core.compaction();
					if (block && Array.isArray(event?.system)) {
						event.system.push({ type: "text", text: block });
					}
				});
			} catch {
				// No compaction seat: the summary is the host's own.
			}
		}

		if (ctx.tool && typeof ctx.tool.hook === "function") {
			try {
				await ctx.tool.hook("execute.before", (event) => {
					const refusal = gateViolation(event?.tool, event?.input);
					if (refusal) {
						throw new Error(refusal);
					}
				});
			} catch {
				// No tool seat: nothing is guarded, and nothing else is lost.
			}

			// The outcome seat: a successful shell/edit call is the evidence
			// the settle question reads, and a save that landed closes it.
			try {
				await ctx.tool.hook("execute.after", (event) => {
					core.observeTool(event);
				});
			} catch {
				// No outcome seat: the question is never asked on this host.
			}
		}

		if (ctx.tool && typeof ctx.tool.transform === "function") {
			try {
				// The catalog the model reads, rewritten in the seat the host
				// re-runs on every refresh. The whole body is wrapped: a measured
				// edge of this seat is that an uncaught throw inside the callback
				// corrupts the turn, so a plugin must never let one out.
				await ctx.tool.transform((editor) => {
					try {
						curateCatalog(editor);
					} catch {
						// The catalog the host built is the catalog the model keeps.
					}
				});
			} catch {
				// No catalog seat: the tools keep the engine's own definitions.
			}
		}

		if (ctx.event && typeof ctx.event.subscribe === "function") {
			try {
				const controller = new AbortController();
				controllers.push(controller);
				const stream = ctx.event.subscribe({
					types: ["session.created", "session.deleted"],
					signal: controller.signal,
				});
				void (async () => {
					try {
						for await (const event of stream) {
							const sessionID = sessionIDOfEvent(event);
							if (event?.type === "session.deleted") {
								// The host's own end signal: the session is over, so its
								// state is written before the plugin forgets it.
								await core.closeSession(sessionID);
								continue;
							}
							await core.startSession(sessionID, directoryOfEvent(event) ?? cwd);
						}
					} catch {
						// The stream ends when the plugin unloads, and a session
						// the engine never heard of is not an error.
					}
				})();
			} catch {
				// No event stream: a session is still registered by its first
				// prompt (see `capture`).
			}
		}

		return async () => {
			for (const controller of controllers) {
				controller.abort();
			}
			return core.shutdown();
		};
	}

	return setup;
}
