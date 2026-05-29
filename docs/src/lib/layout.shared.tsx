import type { BaseLayoutProps } from 'fumadocs-ui/layouts/shared'
import { SiteTitle } from '@/components/chrome/SiteTitle'
import { SocialIcons } from '@/components/chrome/SocialIcons'

export function baseOptions(): BaseLayoutProps {
  return {
    nav: {
      title: <SiteTitle />,
      url: '/',
      children: <SocialIcons />,
    },
    links: [],
    githubUrl: undefined,
    themeSwitch: {
      enabled: false,
    },
  }
}
