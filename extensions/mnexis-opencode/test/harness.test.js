/**
 * The harness's own contract: the fake host must behave like the host it
 * stands for.
 *
 * This is the test that would have caught the dead seat. The fake host used
 * to ignore the `types` filter of `ctx.event.subscribe`, so a test could
 * push any event into the queue and the plugin would consume it — the suite
 * went green on a seat opencode v2.2.5 never publishes. The real host drops
 * an event the plugin did not name; so does this fake.
 */

import assert from "node:assert/strict";
import test from "node:test";

import { fakeHost, waitFor } from "./helpers/host.js";

test("the fake host drops an event the plugin did not ask for", async () => {
	const host = fakeHost();
	const seen = [];
	const stream = host.ctx.event.subscribe({ types: ["session.deleted"] });
	const reader = (async () => {
		for await (const event of stream) {
			seen.push(event?.type);
		}
	})();

	host.push({ type: "session.execution.succeeded", data: { sessionID: "ses_a" } });
	host.push({ type: "session.deleted", data: { sessionID: "ses_a" } });
	await waitFor(() => seen.length > 0);
	host.close();
	await reader;

	assert.deepEqual(seen, ["session.deleted"], "only the named seat is delivered");
});

test("a subscription with no types filter receives the whole stream, as the host does", async () => {
	const host = fakeHost();
	const seen = [];
	const stream = host.ctx.event.subscribe({});
	const reader = (async () => {
		for await (const event of stream) {
			seen.push(event?.type);
		}
	})();

	host.push({ type: "session.execution.started", data: { sessionID: "ses_a" } });
	host.push({ type: "session.usage.updated", data: { sessionID: "ses_a" } });
	await waitFor(() => seen.length > 1);
	host.close();
	await reader;

	assert.deepEqual(seen, ["session.execution.started", "session.usage.updated"]);
});
