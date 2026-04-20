// docs/components/landing/TabbedBuilder.tsx
import { useState, ReactNode } from 'react';

export interface BuilderTab {
  id: string;
  title: string;
  body: ReactNode; // pre-rendered spans with syntax highlighting classes
}

export interface TabbedBuilderProps {
  tabs: BuilderTab[];
  statusLabel?: string;
  statusVariant?: 'built' | 'published';
}

export function TabbedBuilder({ tabs, statusLabel, statusVariant = 'built' }: TabbedBuilderProps) {
  const [activeId, setActiveId] = useState(tabs[0]?.id ?? '');
  const active = tabs.find(t => t.id === activeId) ?? tabs[0];

  return (
    <div className="landing-builder landing-builder-tabbed">
      <div className="landing-builder-head">
        <span className="landing-dot r" />
        <span className="landing-dot y" />
        <span className="landing-dot g" />
        <div className="landing-builder-tabs">
          {tabs.map(t => (
            <button
              key={t.id}
              type="button"
              className={'landing-builder-tab' + (t.id === activeId ? ' active' : '')}
              onClick={() => setActiveId(t.id)}
            >
              {t.title}
            </button>
          ))}
        </div>
        {statusLabel && (
          <span className={'landing-builder-tag landing-tag-' + statusVariant}>{statusLabel}</span>
        )}
      </div>
      <pre className="landing-builder-body">{active?.body}</pre>
    </div>
  );
}
