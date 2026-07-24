import React, { useEffect, useState } from 'react'
import { createPortal } from 'react-dom'
import { IconSparkles } from '../../utils/icons'
import WindowControls from '../app/WindowControls'
import styles from './WriteAssistantWindow.module.css'

const pendingCloseTimers = new WeakMap<Window, ReturnType<typeof setTimeout>>()

interface WriteAssistantWindowProps {
  popup: Window
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
  popup,
  title,
  onClose,
  children,
}) => {
  const [container, setContainer] = useState<HTMLDivElement | null>(null)
  const onCloseRef = React.useRef(onClose)
  onCloseRef.current = onClose

  // Poll for popup closed ¡ª catches cases where beforeunload doesn't fire
  // (e.g. main process closes window via IPC, OS force-close, etc.)
  useEffect(() => {
    const timer = setInterval(() => {
      if (popup.closed) {
        clearInterval(timer)
        onCloseRef.current()
      }
    }, 500)
    return () => clearInterval(timer)
  }, [popup])

  useEffect(() => {
    const pendingClose = pendingCloseTimers.get(popup)
    if (pendingClose) {
      clearTimeout(pendingClose)
      pendingCloseTimers.delete(popup)
    }

    if (popup.closed) {
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
      const closeTimer = setTimeout(() => {
        pendingCloseTimers.delete(popup)
        if (!popup.closed) popup.close()
      }, 0)
      pendingCloseTimers.set(popup, closeTimer)
      setContainer(null)
    }
  }, [onClose, popup, title])

  return container ? createPortal(
    <div className={styles.shell}>
      <header className={styles.titlebar}>
        <div className={styles.title}>
          <IconSparkles size={14} />
          <span>{title}</span>
        </div>
        <WindowControls
          onMinimize={() => window.loom.writeAssistantWindowMinimize()}
          onMaximize={() => window.loom.writeAssistantWindowMaximize()}
          onClose={() => window.loom.writeAssistantWindowClose()}
        />
      </header>
      <main className={styles.content}>{children}</main>
    </div>,
    container,
  ) : null
}
