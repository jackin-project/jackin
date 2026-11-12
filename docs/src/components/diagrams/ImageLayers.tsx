export function ImageLayers() {
  return (
    <figure
      className="jk-layers not-content"
      aria-label="jackin❯ image layers: top derived layer owned by jackin❯, middle role layer from the role repo, bottom shared construct base"
    >
      <ol className="jk-layers-stack">
        <li className="jk-layer jk-layer--derived">
          <div className="jk-layer-title-row">
            <h4 className="jk-layer-title">Derived layer</h4>
            <span className="jk-layer-owner">jackin-managed</span>
          </div>
          <ul className="jk-layer-items">
            <li>Codex, pinned and checksummed</li>
            <li>jackin-start, the main process</li>
            <li>Root-owned setup and preflight hooks</li>
            <li>Image scan</li>
            <li>Docker client, only when the capability is on</li>
            <li>User sandbox</li>
          </ul>
        </li>
        <li className="jk-layer jk-layer--agent">
          <div className="jk-layer-title-row">
            <h4 className="jk-layer-title">Role layer</h4>
            <span className="jk-layer-owner">role repo</span>
          </div>
          <ul className="jk-layer-items">
            <li>Language toolchains</li>
            <li>Development tools</li>
            <li>Custom configuration</li>
          </ul>
        </li>
        <li className="jk-layer jk-layer--base">
          <div className="jk-layer-title-row">
            <h4 className="jk-layer-title">Construct base</h4>
            <span className="jk-layer-owner">built by jackin❯</span>
          </div>
          <ul className="jk-layer-items">
            <li>Debian stable (trixie) base image</li>
            <li>User sandbox, uid 1000</li>
            <li>git, curl, jq, bash</li>
            <li>No setuid bit, no sudo</li>
          </ul>
        </li>
      </ol>
    </figure>
  )
}
