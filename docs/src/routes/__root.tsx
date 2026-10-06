import { createRootRoute, HeadContent, Outlet, Scripts } from '@tanstack/react-router'
import { RootProvider } from 'fumadocs-ui/provider/tanstack'
import SearchDialog from '@/components/search'
import appCss from '@/styles/app.css?url'

const themeScript = `(function(){var root=document.documentElement;var flipped=false;try{var stored=null;try{stored=localStorage.getItem('starlight-theme');if(stored==='auto'){localStorage.removeItem('starlight-theme');stored=null}}catch(e){}var theme;if(stored==='light'||stored==='dark'){theme=stored}else if(typeof window.matchMedia==='function'&&window.matchMedia('(prefers-color-scheme: light)').matches){theme='light'}else{theme='dark'}if(root.getAttribute('data-theme')!==theme){root.setAttribute('data-theme',theme);flipped=true}root.style.colorScheme=theme}catch(e){}if(!flipped)return;root.classList.add('theme-init');function releaseInit(){requestAnimationFrame(function(){requestAnimationFrame(function(){root.classList.remove('theme-init')})})}if(document.readyState==='loading'){document.addEventListener('DOMContentLoaded',releaseInit,{once:true})}else{releaseInit()}})();`

export const Route = createRootRoute({
  head: () => ({
    meta: [
      { charSet: 'utf-8' },
      { name: 'viewport', content: 'width=device-width, initial-scale=1' },
      { title: 'jackin❯ - isolated AI coding agent containers' },
      {
        name: 'description',
        content:
          'Run AI coding agents at full speed inside isolated containers: scoped access, per-agent state, and host boundaries that stay visible.',
      },
      { name: 'theme-color', content: '#0a0a0a', media: '(prefers-color-scheme: dark)' },
      { name: 'theme-color', content: '#ffffff', media: '(prefers-color-scheme: light)' },
    ],
    links: [
      { rel: 'stylesheet', href: appCss },
      { rel: 'icon', href: '/favicon.svg', type: 'image/svg+xml' },
      { rel: 'alternate icon', href: '/favicon.ico', sizes: '16x16 32x32 48x48' },
      { rel: 'apple-touch-icon', sizes: '180x180', href: '/apple-touch-icon.png' },
      { rel: 'icon', type: 'image/png', sizes: '192x192', href: '/icon-192.png' },
      { rel: 'icon', type: 'image/png', sizes: '512x512', href: '/icon-512.png' },
      { rel: 'manifest', href: '/site.webmanifest' },
      { rel: 'sitemap', href: '/sitemap.xml' },
    ],
    scripts: [{ children: themeScript }],
  }),
  component: RootComponent,
})

function RootComponent() {
  return (
    <html lang="en" suppressHydrationWarning>
      <head>
        <HeadContent />
      </head>
      <body>
        <RootProvider
          search={{ SearchDialog }}
          theme={{
            attribute: 'data-theme',
            storageKey: 'starlight-theme',
            defaultTheme: 'dark',
            enableSystem: true,
          }}
        >
          <Outlet />
        </RootProvider>
        <Scripts />
      </body>
    </html>
  )
}
