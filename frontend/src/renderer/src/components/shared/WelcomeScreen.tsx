import { useStore } from '../../stores'

export default function WelcomeScreen() {
  const createSession = useStore((s) => s.createSession)
  const switchSession = useStore((s) => s.switchSession)

  const handleStart = async () => {
    const id = await createSession()
    await switchSession(id)
  }

  return (
    <div className="flex items-center justify-center h-full bg-[var(--bg)]">
      <div className="text-center max-w-md animate-fade-up">
        <div className="w-12 h-12 mx-auto mb-6 rounded-[var(--r-lg)] bg-[rgba(0,227,199,0.06)] border border-[rgba(0,227,199,0.12)] flex items-center justify-center shadow-[0_0_30px_rgba(0,227,199,0.06)]">
          <span className="text-xl font-bold text-[var(--accent)]">L</span>
        </div>
        <h1 className="text-2xl text-[var(--text)] mb-3 tracking-tight font-semibold">
          openLoom
        </h1>
        <p className="text-[var(--text-light)] mb-8 text-[13px] leading-relaxed">
          本地优先的 AI 助理。支持多模型、MCP 工具、
          知识图谱记忆、LSP 代码理解和 Skills 技能系统。
        </p>
        <button
          onClick={handleStart}
          className="px-6 py-2.5 rounded-[var(--r-md)] bg-[var(--accent-light)] text-[var(--accent)] hover:bg-[rgba(var(--accent-rgb),.25)] border border-[var(--border-accent)] text-[13px] font-medium transition-colors"
        >
          开始新对话
        </button>
        <p className="text-[10px] text-[var(--text-muted)] mt-5">
          所有数据存储在本地 SQLite 数据库中
        </p>
      </div>
    </div>
  )
}
