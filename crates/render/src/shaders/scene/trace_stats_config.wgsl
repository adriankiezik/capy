// Global toggle for per-invocation trace-stat counters.
// Set to false for shipping builds to eliminate counter overhead.
const ENABLE_TRACE_STATS: bool = FEATURE_TRACE_STATS;

// Per-pixel LOD debug output is useful in the editor, but it is a full-screen
// storage-buffer write in the trace pass. Keep it off for performance testing.
const ENABLE_LOD_DEBUG: bool = false;
