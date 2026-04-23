// docs/components/landing/ThemeToggle.tsx
// Landing-page theme toggle. Starlight's ThemeSelect override is only
// mounted on docs pages; the landing has its own topnav and needs a
// matching toggle so a user can switch modes without leaving the home
// page. Reuses the 'starlight-theme' localStorage key so the choice
// carries straight over to the docs.
import { useEffect, useState } from 'react';

type Theme = 'dark' | 'light';

const STORAGE_KEY = 'starlight-theme';

function readStoredTheme(): Theme | null {
  try {
    if (typeof localStorage === 'undefined') return null;
    const raw = localStorage.getItem(STORAGE_KEY);
    return raw === 'light' || raw === 'dark' ? raw : null;
  } catch {
    return null;
  }
}

function readRenderedTheme(): Theme {
  // The pre-paint script in index.astro has already set data-theme on
  // <html> — treat that as source of truth for the initial active button.
  if (typeof document === 'undefined') return 'dark';
  return document.documentElement.dataset.theme === 'light' ? 'light' : 'dark';
}

function storeTheme(theme: Theme): void {
  try {
    if (typeof localStorage !== 'undefined') localStorage.setItem(STORAGE_KEY, theme);
  } catch {
    // Private mode / storage-restricted; leave DOM state as-is.
  }
}

function applyTheme(theme: Theme): void {
  const root = document.documentElement;
  root.dataset.theme = theme;
  root.style.colorScheme = theme;
  // Keep the docs toggle visually in sync if the user navigates from
  // here into the docs — the docs ThemeSelect custom element reads
  // aria-pressed on its own <starlight-theme-select> elements too.
  document.querySelectorAll<HTMLElement>('starlight-theme-select button[data-theme-value]').forEach((btn) => {
    const active = (btn as HTMLButtonElement).dataset.themeValue === theme;
    btn.setAttribute('aria-pressed', active ? 'true' : 'false');
  });
}

function onThemePick(theme: Theme): void {
  const doc = document as Document & {
    startViewTransition?: (cb: () => void) => { finished: Promise<void> };
  };
  const reduceMotion =
    typeof window !== 'undefined' &&
    window.matchMedia('(prefers-reduced-motion: reduce)').matches;

  const update = () => {
    applyTheme(theme);
    storeTheme(theme);
  };

  if (!reduceMotion && typeof doc.startViewTransition === 'function') {
    doc.startViewTransition(update);
  } else {
    update();
  }
}

export function ThemeToggle() {
  // SSR: return 'dark' so the server-rendered markup doesn't flip after
  // hydration; the effect below syncs to the actual resolved theme.
  // Important: this component is inside Landing which is client:load,
  // so Astro still SSRs it. Without this guard the React tree would
  // render with window/document references and hydration would break.
  const [theme, setTheme] = useState<Theme>('dark');
  const [mounted, setMounted] = useState(false);

  useEffect(() => {
    // Prefer stored > rendered so the toggle reflects the same choice
    // the pre-paint script resolved. Stored-null means the user is
    // running on OS pref — read rendered as the current display.
    setTheme(readStoredTheme() ?? readRenderedTheme());
    setMounted(true);

    // Cross-page sync: if the user toggles on the docs and this
    // component is still mounted (unlikely, but cheap to support), keep
    // aria-pressed accurate by listening to storage events from other
    // tabs too.
    function onStorage(e: StorageEvent) {
      if (e.key === STORAGE_KEY && (e.newValue === 'light' || e.newValue === 'dark')) {
        setTheme(e.newValue);
      }
    }
    window.addEventListener('storage', onStorage);
    return () => window.removeEventListener('storage', onStorage);
  }, []);

  function handlePick(next: Theme) {
    onThemePick(next);
    setTheme(next);
  }

  return (
    <div className="landing-theme-toggle" role="group" aria-label="Select color theme">
      <button
        type="button"
        aria-label="Use dark theme"
        aria-pressed={mounted && theme === 'dark'}
        onClick={() => handlePick('dark')}
      >
        {/* Moon — matches docs ThemeSelect so the icon set reads as one
            product even with different mount points. */}
        <svg
          width="16"
          height="16"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
          aria-hidden="true"
        >
          <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z" />
        </svg>
      </button>
      <button
        type="button"
        aria-label="Use light theme"
        aria-pressed={mounted && theme === 'light'}
        onClick={() => handlePick('light')}
      >
        {/* Sun */}
        <svg
          width="16"
          height="16"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
          aria-hidden="true"
        >
          <circle cx="12" cy="12" r="4" />
          <path d="M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M4.93 19.07l1.41-1.41M17.66 6.34l1.41-1.41" />
        </svg>
      </button>
    </div>
  );
}
