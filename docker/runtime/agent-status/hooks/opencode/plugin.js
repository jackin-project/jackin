const { spawnSync } = require("child_process");

function report(event, payload = {}) {
  if (!process.env.JACKIN_SESSION_ID) return;
  spawnSync("/jackin/runtime/jackin-capsule", ["report-event", "--event", event, "--payload-stdin"], {
    input: JSON.stringify(payload),
    stdio: ["pipe", "ignore", "ignore"],
  });
}

module.exports = {
  "session.status": (payload) => report("session.status", payload),
  "session.idle": (payload) => report("session.idle", payload),
  "session.error": (payload) => report("session.error", payload),
  "permission.asked": (payload) => report("permission.asked", payload),
  "permission.replied": (payload) => report("permission.replied", payload),
  "tool.execute.before": (payload) => report("tool.execute.before", payload),
  "tool.execute.after": (payload) => report("tool.execute.after", payload),
};
