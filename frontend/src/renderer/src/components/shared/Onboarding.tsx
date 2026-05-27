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
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-[var(--bg)]">
      <div className="max-w-md w-full mx-4 text-center animate-fade-up">
        <div className="mb-8">
          <div className="flex justify-center gap-2 mb-8">
            {STEPS.map((_, i) => (
              <div
                key={i}
                className={`h-1 rounded-full transition-all duration-300 ${
                  i <= step
                    ? 'w-6 bg-[var(--accent)]'
                    : 'w-2 bg-[var(--border)]'
                }`}
              />
            ))}
          </div>
          <h2 className="text-2xl text-[var(--text)] mb-4 tracking-tight font-semibold">
            {STEPS[step].title}
          </h2>
          <p className="text-[var(--text-muted)] text-[13px] leading-relaxed">
            {STEPS[step].description}
          </p>
        </div>

        <div className="flex justify-center gap-3">
          {step > 0 && (
            <button
              onClick={() => setStep(step - 1)}
              className="px-5 py-2.5 rounded-[var(--r-sm)] bg-[var(--bg-card)] text-[var(--text-light)] hover:bg-[rgba(255,255,255,0.04)] border border-[var(--border)] text-sm transition-colors-fast"
            >
              上一步
            </button>
          )}
          {step < STEPS.length - 1 ? (
            <button
              onClick={() => setStep(step + 1)}
              className="px-5 py-2.5 rounded-[var(--r-sm)] bg-[var(--accent-light)] text-[var(--accent)] hover:bg-[rgba(var(--accent-rgb),.25)] border border-[var(--border-accent)] text-sm transition-colors-fast"
            >
              下一步
            </button>
          ) : (
            <button
              onClick={onComplete}
              className="px-5 py-2.5 rounded-[var(--r-sm)] bg-[var(--accent-light)] text-[var(--accent)] hover:bg-[rgba(var(--accent-rgb),.25)] border border-[var(--border-accent)] text-sm font-medium transition-colors-fast"
            >
              开始使用
            </button>
          )}
        </div>
      </div>
    </div>
  )
}
