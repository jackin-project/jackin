export function ImageLayers() {
  return (
    <figure
      className="jk-layers not-content"
      aria-label="jackin❯ image layers: top derived layer owned by jackin❯, middle agent layer from the agent repo, bottom shared construct base"
    >
      <ol className="jk-layers-stack">
        <li className="jk-layer jk-layer--derived">
          <div className="jk-layer-title-row">
            <h4 className="jk-layer-title">Derived layer</h4>
            <span className="jk-layer-owner">jackin-managed</span>
          </div>
          <ul className="jk-layer-items">
            <li>UID/GID remapping</li>
            <li>Claude Code installation</li>
            <li>Pre-launch hook</li>
            <li>Runtime entrypoint</li>
            <li>Plugin bootstrap</li>
          </ul>
        </li>
        <li className="jk-layer jk-layer--agent">
          <div className="jk-layer-title-row">
            <h4 className="jk-layer-title">Agent layer</h4>
            <span className="jk-layer-owner">agent repo</span>
          </div>
          <ul className="jk-layer-items">
            <li>Language runtimes</li>
            <li>Development tools</li>
            <li>Custom configuration</li>
          </ul>
        </li>
        <li className="jk-layer jk-layer--base">
          <div className="jk-layer-title-row">
            <h4 className="jk-layer-title">Construct base</h4>
            <span className="jk-layer-owner">shared</span>
          </div>
          <ul className="jk-layer-items">
            <li>Debian Trixie</li>
            <li>Docker CLI + Compose</li>
            <li>Git, GitHub CLI</li>
            <li>mise, ripgrep, fd, fzf</li>
            <li>zsh + Oh My Zsh</li>
          </ul>
        </li>
      </ol>
    </figure>
  )
}
