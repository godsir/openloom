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
  primary: 'bg-[var(--accent-subtle)] text-[var(--accent)] hover:bg-[var(--accent-medium)] border border-[var(--border-accent)]',
  secondary: 'bg-[rgba(255,255,255,0.03)] text-[var(--text-secondary)] hover:bg-[rgba(255,255,255,0.06)] hover:text-[var(--text)] border border-[var(--border)]',
  ghost: 'bg-transparent text-[var(--text-muted)] hover:bg-[rgba(255,255,255,0.04)] hover:text-[var(--text)]',
  danger: 'bg-[var(--red-light)] text-[var(--red)] hover:bg-[rgba(239,68,68,0.15)] border border-[rgba(239,68,68,0.15)]',
}

const sizes: Record<string, string> = {
  sm: 'px-3 py-1.5 text-[11px]',
  md: 'px-4 py-2 text-[12px]',
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
      className={`rounded-[var(--r-md)] font-medium transition-colors disabled:opacity-30 disabled:cursor-not-allowed ${variants[variant]} ${sizes[size]} ${className}`}
    >
      {children}
    </button>
  )
}
