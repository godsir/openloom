import React, { useEffect, useState } from 'react'
import { createPortal } from 'react-dom'
import styles from './WriteAssistantWindow.module.css'

const WINDOW_NAME = 'openloom-write-assistant'
const WINDOW_FEATURES = 'popup=yes,width=380,height=640,resizable=yes'

interface WriteAssistantWindowProps {
  title: string
  onClose: () => void
  children: React.ReactNode
}

function copyDocumentStyles(target: Document): void {
  const base = target.createElement('base')
  base.href = document.baseURI
  target.head.appendChild(base)
  document.head.querySelectorAll('style, link[rel="stylesheet"]').forEach((node) => {
    target.head.appendChild(node.cloneNode(true))
  })
}

function syncRootAppearance(target: Document): void {
  for (const attribute of document.documentElement.attributes) {
    target.documentElement.setAttribute(attribute.name, attribute.value)
  }
  target.body.className = document.body.className
}

export const WriteAssistantWindow: React.FC<WriteAssistantWindowProps> = ({
  title,
  onClose,
  children,
}) => {
  const [container, setContainer] = useState<HTMLDivElement | null>(null)

  useEffect(() => {
    const popup = window.open('about:blank', WINDOW_NAME, WINDOW_FEATURES)
    if (!popup) {
      onClose()
      return
    }

    popup.document.title = title
    popup.document.body.replaceChildren()
    copyDocumentStyles(popup.document)
    syncRootAppearance(popup.document)

    const root = popup.document.createElement('div')
    root.className = styles.windowRoot
    popup.document.body.appendChild(root)
    setContainer(root)

    const handleClosed = (): void => onClose()
    popup.addEventListener('beforeunload', handleClosed)
    const appearanceObserver = new MutationObserver(() => syncRootAppearance(popup.document))
    appearanceObserver.observe(document.documentElement, { attributes: true })

    return () => {
      appearanceObserver.disconnect()
      popup.removeEventListener('beforeunload', handleClosed)
      if (!popup.closed) popup.close()
      setContainer(null)
    }
  }, [onClose, title])

  return container ? createPortal(children, container) : null
}
