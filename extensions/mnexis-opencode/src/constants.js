/** The plugin's stable id, in both models. */
export const PLUGIN_ID = "mnexis";

/** Set to `0` to stop the plugin from pointing at the door. */
export const POINTER_ENV = "MNEXIS_OPENCODE_POINTER";

/** How many live state records the compaction block may carry. */
export const STATE_LIMIT = 8;

/** How many characters of state content the compaction block may carry. */
export const STATE_CHARS = 3000;

/** How much of the previous session's summary the pointer may carry. */
export const SUMMARY_CHARS = 240;
