import { Callout, type CalloutType } from 'fumadocs-ui/components/callout'
import { Step, Steps as FumaSteps } from 'fumadocs-ui/components/steps'
import { Tab, Tabs as FumaTabs } from 'fumadocs-ui/components/tabs'
import { Card as FumaCard, Cards } from 'fumadocs-ui/components/card'
import type { ReactNode } from 'react'

export function Aside({
  type = 'note',
  title,
  children,
}: {
  type?: 'note' | 'tip' | 'caution' | 'danger' | 'warning'
  title?: ReactNode
  children: ReactNode
}) {
  const mapped: CalloutType = type === 'caution' || type === 'warning' ? 'warn' : type === 'danger' ? 'error' : 'info'
  return (
    <Callout className={`jk-aside jk-aside--${type}`} type={mapped} title={title}>
      {children}
    </Callout>
  )
}

export function Steps({ children }: { children: ReactNode }) {
  return <FumaSteps>{children}</FumaSteps>
}

export function Tabs({ children }: { children: ReactNode }) {
  const items = Array.isArray(children)
    ? children
        .map((child) => (typeof child === 'object' && child !== null && 'props' in child ? child.props.label : undefined))
        .filter((item): item is string => typeof item === 'string')
    : []

  return <FumaTabs items={items}>{children}</FumaTabs>
}

export function TabItem({ label, children }: { label: string; children: ReactNode }) {
  return <Tab value={label}>{children}</Tab>
}

export function Card({
  title,
  children,
  href,
}: {
  title: ReactNode
  children?: ReactNode
  href?: string
}) {
  return (
    <Cards>
      <FumaCard title={title} description={children} href={href} />
    </Cards>
  )
}

export { Step }
