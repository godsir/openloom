import { useStore } from '../../stores'

export default function WelcomeScreen() {
  const createSession = useStore((s) => s.createSession)
  const switchSession = useStore((s) => s.switchSession)

  const handleStart = async () => {
    const id = await createSession()
    await switchSession(id)
  }

  return (
    <div className="flex items-center justify-center h-full">
      <div className="text-center max-w-md">
        <h1 className="text-3xl font-bold mb-3">欢迎使用 openLoom</h1>
        <p className="text-zinc-400 mb-6 text-sm leading-relaxed">
          你的本地优先 AI 助理。所有数据存储在本地，支持多模型、MCP 工具、
          知识图谱记忆、LSP 代码理解和 Skills 技能系统。
        </p>
        <button
          onClick={handleStart}
          className="px-6 py-3 bg-blue-600 text-white rounded-lg hover:bg-blue-500 transition-colors text-sm font-medium"
        >
          开始新对话
        </button>
        <p className="text-xs text-zinc-600 mt-4">
          会话将自动保存在本地 SQLite 数据库中
        </p>
      </div>
    </div>
  )
}
