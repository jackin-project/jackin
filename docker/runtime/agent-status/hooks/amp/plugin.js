const { spawnSync } = require("child_process");

function report(event, payload = {}) {
  if (!process.env.JACKIN_SESSION_ID) return;
  spawnSync("/jackin/runtime/jackin-capsule", ["report-event", "--event", event, "--payload-stdin"], {
    input: JSON.stringify(payload),
    stdio: ["pipe", "ignore", "ignore"],
  });
}

module.exports = {
  "agent.start": (payload) => report("agent.start", payload),
  "tool.call": (payload) => report("tool.call", payload),
  "tool.result": (payload) => report("tool.result", payload),
  "agent.end": (payload) => report("agent.end", payload),
};
