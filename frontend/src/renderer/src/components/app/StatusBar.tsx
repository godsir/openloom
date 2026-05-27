import { useStore } from '../../stores'

export default function StatusBar() {
  const wsState = useStore(s => s.wsState)

  return (
    <div className="flex items-center h-[var(--statusbar-h)] shrink-0 bg-[var(--bg)] border-t border-[var(--border-light)] px-3 text-[10px] text-[var(--text-muted)]">
      <div className="flex items-center gap-1.5">
        <span className="w-1 h-1 rounded-full"
          style={{
            background: wsState==='connected' ? 'var(--accent)' : wsState==='reconnecting' ? 'var(--amber)' : 'var(--red)',
            boxShadow: wsState==='connected' ? '0 0 4px rgba(0,227,199,0.4)' : 'none',
          }} />
        <span>{wsState==='connected'?'已连接':wsState==='reconnecting'?'重连中':'离线'}</span>
      </div>
    </div>
  )
}
