import { RouterClient } from '@tanstack/react-router/ssr/client'
import { hydrateRoot } from 'react-dom/client'
import { getRouter } from './router'

hydrateRoot(document, <RouterClient router={getRouter()} />)
