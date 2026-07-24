import { useEffect, useRef } from 'react'

interface MenuKeyboardOptions {
  open: boolean
  itemCount: number
  activeIndex: number
  setActiveIndex: (updater: (prev: number) => number) => void
  onSelect: (index: number) => void
  onClose: () => void
}

/**
 * 下拉菜单键盘导航：↑/↓ 循环移动高亮、Enter 选中、Esc 关闭。
 *
 * 复用 SlashCommandMenu 的成熟模式，统一此前各弹层（Select / EntitySelector 等）
 * 参差不齐甚至完全缺失的键盘支持。挂在 document 级 keydown 上，配合"活动项
 * 高亮"渲染即可，无需逐项聚焦。
 */
export function useMenuKeyboard({
  open,
  itemCount,
  activeIndex,
  setActiveIndex,
  onSelect,
  onClose,
}: MenuKeyboardOptions, eventDocument: Document = document) {
  // 用 ref 持有最新回调，effect 仅在 open/itemCount 变化时重绑，避免抖动
  const stateRef = useRef({ activeIndex, itemCount, setActiveIndex, onSelect, onClose })
  stateRef.current = { activeIndex, itemCount, setActiveIndex, onSelect, onClose }

  useEffect(() => {
    if (!open) return
    const onKey = (e: KeyboardEvent) => {
      const s = stateRef.current
      if (e.key === 'ArrowDown') {
        e.preventDefault()
        s.setActiveIndex(prev => (prev + 1) % Math.max(s.itemCount, 1))
      } else if (e.key === 'ArrowUp') {
        e.preventDefault()
        s.setActiveIndex(prev => (prev - 1 + s.itemCount) % Math.max(s.itemCount, 1))
      } else if (e.key === 'Enter') {
        e.preventDefault()
        s.onSelect(s.activeIndex)
      } else if (e.key === 'Escape') {
        e.preventDefault()
        s.onClose()
      }
    }
    eventDocument.addEventListener('keydown', onKey)
    return () => eventDocument.removeEventListener('keydown', onKey)
  }, [open, itemCount, eventDocument])
}

/**
 * 点击外部关闭：统一为 document + pointerdown。
 *
 * 此前 6 处弹层各写各的外点关闭 effect（有的 click、有的 mousedown、有的带
 * setTimeout(0) 竞态）。这里收敛为一个 hook：enabled=false 不绑定；延迟一帧
 * 绑定，避免"打开菜单的那次点击"立刻把它关掉。
 */
export function useClickOutside(
  isInside: (target: Node) => boolean,
  onClickOutside: () => void,
  enabled: boolean,
  eventDocument: Document = document,
) {
  const isInsideRef = useRef(isInside)
  isInsideRef.current = isInside
  const cbRef = useRef(onClickOutside)
  cbRef.current = onClickOutside

  useEffect(() => {
    if (!enabled) return
    const handler = (e: PointerEvent) => {
      if (isInsideRef.current(e.target as Node)) return
      cbRef.current()
    }
    const id = setTimeout(() => eventDocument.addEventListener('pointerdown', handler), 0)
    return () => {
      clearTimeout(id)
      eventDocument.removeEventListener('pointerdown', handler)
    }
  }, [enabled, eventDocument])
}
