/**
 * Default-to-dark theme initializer.
 *
 * Vocs's built-in /initializeTheme.iife.js falls back to
 * prefers-color-scheme when localStorage['vocs.theme'] is empty, which
 * means users with a light-mode OS preference would see the docs in
 * light by default. We want dark as the brand default, but still honor
 * an explicit user choice (via the ThemeToggle in the sidebar).
 *
 * This script:
 *   1. Runs synchronously from <head> (blocking, before first paint).
 *   2. If no user preference is stored, sets it to 'dark' and applies
 *      the .dark class immediately to avoid any light-mode flash.
 *   3. If a preference is stored ('light' or 'dark'), respects it.
 */
(function () {
  try {
    var html = document.documentElement;
    var stored = localStorage.getItem('vocs.theme');
    if (stored !== 'light' && stored !== 'dark') {
      localStorage.setItem('vocs.theme', 'dark');
      if (!html.classList.contains('dark')) html.classList.add('dark');
    }
  } catch (_) {
    // localStorage not available (SSR, privacy mode) — ignore.
  }
})();
