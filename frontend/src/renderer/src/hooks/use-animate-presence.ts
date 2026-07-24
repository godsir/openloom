import { useState, useEffect, useRef } from 'react'

interface AnimationConfig {
  enterDuration?: number
  exitDuration?: number
}

export function useAnimatePresence(
  visible: boolean,
  config?: AnimationConfig,
) {
  const { enterDuration = 200, exitDuration = 200 } = config || {}
  const [state, setState] = useState<'enter' | 'idle' | 'exit' | 'hidden'>(
    visible ? 'enter' : 'hidden',
  )
  const timerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)

  useEffect(() => {
    if (visible) {
      setState('enter')
      timerRef.current = setTimeout(() => setState('idle'), enterDuration)
    } else {
      if (state === 'hidden') return
      setState('exit')
      timerRef.current = setTimeout(() => setState('hidden'), exitDuration)
    }
    return () => {
      if (timerRef.current) clearTimeout(timerRef.current)
    }
  }, [visible])

  return {
    state,
    isVisible: state !== 'hidden',
    className:
      state === 'enter'
        ? 'animate-enter'
        : state === 'exit'
          ? 'animate-exit'
          : '',
  }
}
