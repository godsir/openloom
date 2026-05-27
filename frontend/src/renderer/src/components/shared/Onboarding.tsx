import { useState } from 'react'

interface Step {
  title: string
  description: string
}

const STEPS: Step[] = [
  {
    title: '欢迎来到 openLoom',
    description:
      'openLoom 是一个本地优先的私人 AI 助理。所有对话、记忆和配置都存储在你的电脑上。',
  },
  {
    title: '选择模型',
    description:
      'openLoom 支持多种 AI 模型。你可以在设置中配置云端模型（Anthropic、OpenAI、DeepSeek）或本地模型（LM Studio、Ollama）。',
  },
  {
    title: '开始对话',
    description:
      '点击 "开始" 创建你的第一个对话。你可以随时在左侧边栏管理会话。',
  },
]

export default function Onboarding({
  onComplete,
}: {
  onComplete: () => void
}) {
  const [step, setStep] = useState(0)

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-zinc-950">
      <div className="max-w-md w-full mx-4 text-center">
        <div className="mb-8">
          <div className="flex justify-center gap-2 mb-6">
            {STEPS.map((_, i) => (
              <div
                key={i}
                className={`w-2 h-2 rounded-full ${
                  i <= step ? 'bg-blue-500' : 'bg-zinc-700'
                }`}
              />
            ))}
          </div>
          <h2 className="text-xl font-semibold mb-3">{STEPS[step].title}</h2>
          <p className="text-zinc-400 text-sm leading-relaxed">
            {STEPS[step].description}
          </p>
        </div>

        <div className="flex justify-center gap-3">
          {step > 0 && (
            <button
              onClick={() => setStep(step - 1)}
              className="px-5 py-2 bg-zinc-800 text-zinc-300 rounded-lg hover:bg-zinc-700 text-sm"
            >
              上一步
            </button>
          )}
          {step < STEPS.length - 1 ? (
            <button
              onClick={() => setStep(step + 1)}
              className="px-5 py-2 bg-blue-600 text-white rounded-lg hover:bg-blue-500 text-sm"
            >
              下一步
            </button>
          ) : (
            <button
              onClick={onComplete}
              className="px-5 py-2 bg-blue-600 text-white rounded-lg hover:bg-blue-500 text-sm font-medium"
            >
              开始使用
            </button>
          )}
        </div>
      </div>
    </div>
  )
}
