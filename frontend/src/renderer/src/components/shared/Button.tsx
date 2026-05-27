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
  primary: 'bg-blue-600 text-white hover:bg-blue-500',
  secondary: 'bg-zinc-700 text-zinc-200 hover:bg-zinc-600',
  ghost: 'bg-transparent text-zinc-400 hover:bg-zinc-800 hover:text-zinc-200',
  danger: 'bg-red-600 text-white hover:bg-red-500',
}

const sizes: Record<string, string> = {
  sm: 'px-2 py-1 text-xs',
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
      className={`rounded-lg transition-colors disabled:opacity-40 disabled:cursor-not-allowed font-medium ${variants[variant]} ${sizes[size]} ${className}`}
    >
      {children}
    </button>
  )
}
