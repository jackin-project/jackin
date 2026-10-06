import type { BaseLayoutProps } from 'fumadocs-ui/layouts/shared'
import { SiteTitle } from '@/components/chrome/SiteTitle'

export function baseOptions(): BaseLayoutProps {
  return {
    nav: {
      title: <SiteTitle />,
      url: '/',
    },
    links: [],
    githubUrl: undefined,
    themeSwitch: {
      enabled: false,
    },
  }
}
