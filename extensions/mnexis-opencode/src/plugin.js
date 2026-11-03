// --- the plugin, in both shapes -------------------------------------------

/**
 * Build the plugin.
 *
 * `exec`/`binary`/`env`/`logger`/`registry` are injectable so the tests never
 * need the binary, a profile or a host.
 */
export function createPlugin({ exec, binary, env = process.env, logger, registry } = {}) {
	// One registry per plugin instance: the client spawns the children, and the
	// shutdown is the one that kills what is still in flight.
	const children = registry ?? createChildRegistry();
	const coreFor = (cwd, hostLogger) =>
		createCore({
			cwd,
			env,
			client: createClient({ exec, binary, env, registry: children }),
			logger: logger ?? hostLogger,
			registry: children,
		});
	const setup = createV2Entry(coreFor);
	const server = createV1Entry(coreFor);

	return { id: PLUGIN_ID, setup, server };
}

/**
 * The plugin both hosts load.
 *
 * V1 detect-mode reads `id` and calls `server`; V2 decodes `{ id, setup }` and
 * drops the rest. The tests build their own through `createPlugin`, so they can
 * inject the exec that the member above does not.
 */
const plugin = createPlugin();

export default plugin;
