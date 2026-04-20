// docs/layout.tsx
// Consumer Layout wrapper picked up by Vocs via virtual:consumer-components.
// Runs on every page (docs + landing), so this is where global side-effects like
// font-link injection belong.
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

export default function Layout({ children }: { children: ReactNode }) {
  useEffect(() => {
    ensureFontsLink();
  }, []);

  return <>{children}</>;
}
