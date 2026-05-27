import { type ReactNode } from 'react'

interface ButtonProps {
  children: ReactNode
  onClick?: () => void
  disabled?: boolean
  variant?: 'primary' | 'secondary' | 'ghost' | 'danger'
  size?: 'sm' | 'md'
  className?: string
  title?: string
  'aria-label'?: string
}

const variants: Record<string, string> = {
  primary: 'bg-[var(--accent-light)] text-[var(--accent)] hover:bg-[rgba(var(--accent-rgb),.25)] border border-[var(--border-accent)]',
  secondary: 'bg-[var(--bg-card)] text-[var(--text-light)] hover:bg-[rgba(255,255,255,0.04)] hover:text-[var(--text)] border border-[var(--border)]',
  ghost: 'bg-transparent text-[var(--text-muted)] hover:bg-[var(--bg-card)] hover:text-[var(--text)]',
  danger: 'bg-[var(--red-light)] text-[var(--red)] hover:bg-red-500/20 border border-[rgba(239,68,68,0.15)]',
}

const sizes: Record<string, string> = {
  sm: 'px-2.5 py-1 text-xs',
  md: 'px-4 py-2 text-sm',
}

export default function Button({
  children,
  onClick,
  disabled,
  variant = 'secondary',
  size = 'md',
  className = '',
  title,
  'aria-label': ariaLabel,
}: ButtonProps) {
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      title={title}
      aria-label={ariaLabel}
      className={`rounded-[var(--r-sm)] font-medium transition-colors disabled:opacity-30 disabled:cursor-not-allowed ${variants[variant]} ${sizes[size]} ${className}`}
    >
      {children}
    </button>
  )
}
