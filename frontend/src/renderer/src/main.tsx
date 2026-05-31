import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import App from './App'
import './services/pet-commands' // always-on pet context menu handler
import './styles/base.css'
import './themes/light.css'
import './themes/midnight.css'
import './themes/warm-paper.css'
import './themes/neon-pink.css'
import './themes/ember.css'
import './themes/navy-gold.css'
import './themes/umber-cream.css'

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
)
