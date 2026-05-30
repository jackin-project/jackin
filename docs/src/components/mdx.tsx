import defaultMdxComponents from 'fumadocs-ui/mdx'
import type { MDXComponents } from 'mdx/types'
import { ArchitectureDiagram } from './diagrams/ArchitectureDiagram'
import { ImageLayers } from './diagrams/ImageLayers'
import { EarlyDevelopmentNotice } from './EarlyDevelopmentNotice'
import { RepoFile } from './RepoFile'
import { Aside, Card, Step, Steps, TabItem, Tabs } from './mdx/legacy'

export function getMDXComponents(components?: MDXComponents) {
  return {
    ...defaultMdxComponents,
    Aside,
    ArchitectureDiagram,
    Card,
    EarlyDevelopmentNotice,
    ImageLayers,
    RepoFile,
    Step,
    Steps,
    TabItem,
    Tabs,
    ...components,
  } satisfies MDXComponents
}

export const useMDXComponents = getMDXComponents

declare global {
  type MDXProvidedComponents = ReturnType<typeof getMDXComponents>
}
