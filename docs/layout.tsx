// docs/layout.tsx
// Consumer Layout wrapper picked up by Vocs via virtual:consumer-components.
// Runs on every page (docs + landing), so this is where global side-effects
// like font-link injection and docs-outline enhancement belong.
import { useEffect, type ReactNode } from 'react';

const FONTS_HREF =
  'https://fonts.googleapis.com/css2?family=JetBrains+Mono:wght@400;500;600&family=Inter:wght@400;500;600;700;800;900&family=Fraunces:opsz,wght@9..144,400;9..144,500;9..144,700&display=swap';

function ensureFontsLink() {
  if (typeof document === 'undefined') return;
  if (document.querySelector('link[data-site-fonts]')) return;

  const pc1 = document.createElement('link');
  pc1.rel = 'preconnect';
  pc1.href = 'https://fonts.googleapis.com';
  pc1.dataset.siteFonts = 'preconnect-1';
  document.head.appendChild(pc1);

  const pc2 = document.createElement('link');
  pc2.rel = 'preconnect';
  pc2.href = 'https://fonts.gstatic.com';
  pc2.crossOrigin = 'anonymous';
  pc2.dataset.siteFonts = 'preconnect-2';
  document.head.appendChild(pc2);

  const sheet = document.createElement('link');
  sheet.rel = 'stylesheet';
  sheet.href = FONTS_HREF;
  sheet.dataset.siteFonts = 'stylesheet';
  document.head.appendChild(sheet);
}

/**
 * Enhance Vocs's outline with Tempo's sliding active-item indicator
 * (data-v-outline-indicator pattern). Injects a single indicator div
 * that translates + resizes to the currently-active outline link.
 */
function enhanceOutline() {
  if (typeof document === 'undefined') return;
  const list = document.querySelector<HTMLElement>('.vocs_Outline ul');
  if (!list) return;
  if (list.querySelector('[data-outline-indicator]')) return;

  list.style.position = 'relative';
  const indicator = document.createElement('div');
  indicator.setAttribute('data-outline-indicator', '');
  list.appendChild(indicator);

  let raf = 0;
  function update() {
    cancelAnimationFrame(raf);
    raf = requestAnimationFrame(() => {
      // Vocs 1.4.1 sets data-active="true" on the <a> (.vocs_Outline_link).
      const active =
        list!.querySelector<HTMLElement>('a[data-active="true"]') ||
        list!.querySelector<HTMLElement>('a[aria-current="true"]') ||
        list!.querySelector<HTMLElement>('a.vocs_Outline_item_active');
      if (!active) {
        indicator.style.opacity = '0';
        return;
      }
      const rect = active.getBoundingClientRect();
      const parentRect = list!.getBoundingClientRect();
      indicator.style.opacity = '1';
      indicator.style.transform = `translateY(${rect.top - parentRect.top}px)`;
      indicator.style.height = `${rect.height}px`;
    });
  }

  update();
  const mo = new MutationObserver(update);
  mo.observe(list, {
    attributes: true,
    subtree: true,
    attributeFilter: ['data-active', 'aria-current', 'class'],
  });
  window.addEventListener('scroll', update, { passive: true });
  window.addEventListener('resize', update, { passive: true });
}

/**
 * Rename Vocs's "Ask in ChatGPT" CTA to "Ask AI" (Tempo style). The
 * provider icon + dropdown are preserved; only the visible label is
 * swapped. Re-run on route changes since the button re-renders.
 */
function renameAskCta() {
  if (typeof document === 'undefined') return;
  const btn = document.querySelector<HTMLElement>('.vocs_AiCtaDropdown_buttonLeft');
  if (!btn || btn.dataset.renamed === 'true') return;
  for (const node of Array.from(btn.childNodes)) {
    if (node.nodeType === Node.TEXT_NODE && node.nodeValue?.trim().length) {
      node.nodeValue = 'Ask AI';
    }
  }
  btn.dataset.renamed = 'true';
}

export default function Layout({ children }: { children: ReactNode }) {
  useEffect(() => {
    ensureFontsLink();
    // Wait a tick so Vocs's outline + AI CTA have mounted before we enhance.
    const t = setTimeout(() => {
      enhanceOutline();
      renameAskCta();
    }, 0);
    return () => clearTimeout(t);
  }, []);

  return <>{children}</>;
}
