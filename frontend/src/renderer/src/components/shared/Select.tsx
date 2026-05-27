interface SelectProps<T extends string> {
  value: T
  options: { value: T; label: string }[]
  onChange: (value: T) => void
  className?: string
}

export default function Select<T extends string>({
  value,
  options,
  onChange,
  className = '',
}: SelectProps<T>) {
  return (
    <select
      value={value}
      onChange={(e) => onChange(e.target.value as T)}
      className={`bg-[var(--bg-card)] text-[var(--text-light)] text-sm rounded-[var(--r-input)] px-3 py-1.5 outline-none border border-[var(--border)] focus:border-[var(--border-accent)] cursor-pointer transition-colors ${className}`}
    >
      {options.map((opt) => (
        <option key={opt.value} value={opt.value}>
          {opt.label}
        </option>
      ))}
    </select>
  )
}
