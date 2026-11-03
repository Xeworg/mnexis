/**
 * The text a submitted user message carries, from its parts (V1's
 * `chat.message` output).
 *
 * A part the host wrote is `synthetic: true` — an agent-part hint, an
 * auto-continue — and is not something the person said. A message whose every
 * text part is synthetic has nothing to capture.
 *
 * @param {unknown} parts
 * @returns {string}
 */
export function promptTextFromParts(parts) {
	if (!Array.isArray(parts)) {
		return "";
	}
	const texts = [];
	for (const part of parts) {
		if (!part || typeof part !== "object") continue;
		if (part.type !== "text") continue;
		if (part.synthetic === true) continue;
		if (typeof part.text !== "string") continue;
		texts.push(part.text);
	}
	return texts.join("\n").trim() === "" ? "" : texts.join("\n");
}

/** True when the host's message is not something the person typed. */
export function isHumanMessage(message) {
	if (!message || typeof message !== "object") {
		return true;
	}
	return message.role === undefined || message.role === "user";
}

/**
 * The session id an event names, in either host's shape.
 *
 * V1: `{ type: "session.created", properties: { info: { id } } }` for the events
 * that name a whole session, and `{ type: "session.idle", properties: {
 * sessionID } }` for the ones that name it by id.
 * V2: `{ type: "session.created", data: { sessionID } }`
 *
 * @param {unknown} event
 * @returns {string | null}
 */
export function sessionIDOfEvent(event) {
	if (!event || typeof event !== "object") {
		return null;
	}
	const fromData = event.data && typeof event.data.sessionID === "string" ? event.data.sessionID : null;
	if (fromData) {
		return fromData;
	}
	const properties = event.properties && typeof event.properties === "object" ? event.properties : null;
	if (properties && typeof properties.sessionID === "string" && properties.sessionID !== "") {
		return properties.sessionID;
	}
	const info = properties && properties.info;
	if (info && typeof info.id === "string" && info.id !== "") {
		return info.id;
	}
	return null;
}

/** The directory an event names, when it names one. */
export function directoryOfEvent(event) {
	if (!event || typeof event !== "object") {
		return null;
	}
	const info = event.properties && event.properties.info;
	if (info && typeof info.directory === "string" && info.directory !== "") {
		return info.directory;
	}
	const location = event.location;
	if (location && typeof location.directory === "string" && location.directory !== "") {
		return location.directory;
	}
	return null;
}
