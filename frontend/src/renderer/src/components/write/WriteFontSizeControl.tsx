import React from 'react'
import { useWriteStore } from '../../stores/write'
import styles from './WriteFontSizeControl.module.css'

const MIN_FONT_SIZE = 10
const MAX_FONT_SIZE = 32

/**
 * 字体大小控制 — 通过 +/- 按钮调节编辑器字体大小。
 * 从 useWriteStore 读取 fontSize，通过 setFontSize 更新。
 */
export default function WriteFontSizeControl() {
  const fontSize = useWriteStore((s) => s.fontSize)
  const setFontSize = useWriteStore((s) => s.setFontSize)

  const decrease = () => setFontSize(Math.max(MIN_FONT_SIZE, fontSize - 1))
  const increase = () => setFontSize(Math.min(MAX_FONT_SIZE, fontSize + 1))

  return (
    <div className={styles.root}>
      <button
        className={styles.btn}
        onClick={decrease}
        disabled={fontSize <= MIN_FONT_SIZE}
        title="缩小字体"
      >
        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
          <line x1="5" y1="12" x2="19" y2="12" />
        </svg>
      </button>

      <span className={styles.label}>{fontSize}</span>

      <button
        className={styles.btn}
        onClick={increase}
        disabled={fontSize >= MAX_FONT_SIZE}
        title="放大字体"
      >
        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
          <line x1="12" y1="5" x2="12" y2="19" />
          <line x1="5" y1="12" x2="19" y2="12" />
        </svg>
      </button>
    </div>
  )
}
