// docs/components/landing/VocabularyDictionary.tsx
import { useState, useEffect, useRef } from 'react';
import { vocabularyEntries } from './vocabularyData';

export function VocabularyDictionary() {
  const sectionRef = useRef<HTMLElement>(null);
  const [activeIdx, setActiveIdx] = useState(0);

  useEffect(() => {
    const section = sectionRef.current;
    if (!section) return;

    let rafId = 0;
    let ticking = false;
    function onScroll() {
      if (ticking) return;
      ticking = true;
      rafId = requestAnimationFrame(() => {
        const rect = section!.getBoundingClientRect();
        const vh = window.innerHeight;
        const h = section!.offsetHeight;
        const scrollDist = h - vh;
        if (scrollDist <= 0) { setActiveIdx(0); ticking = false; return; }
        const scrolled = Math.max(0, -rect.top);
        const p = Math.max(0, Math.min(0.999, scrolled / scrollDist));
        const idx = Math.min(vocabularyEntries.length - 1, Math.floor(p * vocabularyEntries.length));
        setActiveIdx(idx);
        ticking = false;
      });
    }
    window.addEventListener('scroll', onScroll, { passive: true });
    window.addEventListener('resize', onScroll, { passive: true });
    onScroll();
    return () => {
      if (rafId) cancelAnimationFrame(rafId);
      window.removeEventListener('scroll', onScroll);
      window.removeEventListener('resize', onScroll);
    };
  }, []);

  function jumpTo(i: number) {
    const section = sectionRef.current;
    if (!section) return;
    const vh = window.innerHeight;
    const scrollDist = section.offsetHeight - vh;
    if (scrollDist <= 0) return;
    const target = section.offsetTop + ((i + 0.5) / vocabularyEntries.length) * scrollDist;
    window.scrollTo({ top: target, behavior: 'smooth' });
  }

  const e = vocabularyEntries[activeIdx];

  return (
    <section id="why" ref={sectionRef} className="landing-section landing-voc-scroll-section">
      <div className="landing-voc-sticky">
        <div className="landing-shell">
          <div className="landing-sec-label">02 · Vocabulary</div>
          <h2 className="landing-sec-title">The vocabulary <span className="accent">is</span> the product.</h2>
          <p className="landing-sec-intro">Every command in jackin' maps to a concept from The Matrix — not for fun, but because the Matrix mental model is the shortest path to understanding what the tool does.</p>

          <div className="landing-voc">
            <div className="landing-voc-list">
              {vocabularyEntries.map((entry, i) => (
                <button
                  type="button"
                  key={entry.id}
                  className={'landing-voc-item' + (i === activeIdx ? ' active' : '')}
                  onClick={() => jumpTo(i)}
                >
                  <span className="num">{entry.id}</span>
                  <span className="word">{entry.term}</span>
                </button>
              ))}
            </div>
            <div key={activeIdx} className="landing-voc-detail landing-fade">
              <div className="landing-detail-word">{e.term}</div>
              <div className="landing-detail-pos">{e.pos}</div>
              <p className="landing-detail-def">
                <span className="lead">—</span>
                {e.def.map((seg, i) => seg.b ? <strong key={i}>{seg.t}</strong> : <span key={i}>{seg.t}</span>)}
              </p>
              {e.cmd && (
                <div className="landing-detail-cmd">
                  <span className="lbl">{e.cmdLabel}</span>
                  {e.cmd}
                </div>
              )}
            </div>
          </div>
        </div>
      </div>
    </section>
  );
}
