'use client'

import { useEffect, useState } from 'react'

type Theme = 'dark' | 'light'

const storageKey = 'starlight-theme'

function readTheme(): Theme {
  if (typeof document === 'undefined') return 'dark'
  return document.documentElement.dataset.theme === 'light' ? 'light' : 'dark'
}

function storeTheme(theme: Theme) {
  try {
    localStorage.setItem(storageKey, theme)
  } catch {
    // Storage may be unavailable; the DOM state still updates.
  }
}

function applyTheme(theme: Theme) {
  document.documentElement.dataset.theme = theme
  document.documentElement.style.colorScheme = theme
  storeTheme(theme)
}

export function ThemeToggle() {
  const [theme, setTheme] = useState<Theme>('dark')

  useEffect(() => {
    setTheme(readTheme())
  }, [])

  function pick(next: Theme) {
    const doc = document as Document & {
      startViewTransition?: (cb: () => void) => { finished: Promise<void> }
    }
    const reduceMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches
    const update = () => {
      applyTheme(next)
      setTheme(next)
    }

    if (!reduceMotion && typeof doc.startViewTransition === 'function') {
      doc.startViewTransition(update)
    } else {
      update()
    }
  }

  return (
    <div className="jk-theme-toggle" role="group" aria-label="Select theme">
      <button type="button" aria-label="Dark" aria-pressed={theme === 'dark'} onClick={() => pick('dark')}>
        <svg aria-hidden="true" viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" strokeWidth="2">
          <path d="M20.9 13.6A8 8 0 0 1 10.4 3.1a8.5 8.5 0 1 0 10.5 10.5Z" />
        </svg>
      </button>
      <button type="button" aria-label="Light" aria-pressed={theme === 'light'} onClick={() => pick('light')}>
        <svg aria-hidden="true" viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" strokeWidth="2">
          <circle cx="12" cy="12" r="4" />
          <path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4" />
        </svg>
      </button>
    </div>
  )
}
