// docs/components/landing/FocusCallout.tsx
export function FocusCallout() {
  return (
    <div className="landing-focus-callout">
      <div className="landing-focus-card">
        <div className="landing-focus-label">Kitchen-Sink Agent</div>
        <div className="landing-focus-lines">
          <span>Every toolchain.</span>
          <span>Every plugin.</span>
          <span>Every convention.</span>
        </div>
        <p className="landing-focus-note">Too much context — worse decisions.</p>
      </div>
      <div className="landing-focus-card focus-role">
        <div className="landing-focus-label">Role-Specific Agent</div>
        <div className="landing-focus-lines">
          <span>Only relevant tools.</span>
          <span>Only matching plugins.</span>
          <span>Only applicable conventions.</span>
        </div>
        <p className="landing-focus-note">Focused context — better results, faster.</p>
      </div>
    </div>
  );
}
