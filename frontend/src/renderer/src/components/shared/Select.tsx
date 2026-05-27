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
      className={`bg-zinc-800 text-zinc-200 text-sm rounded-lg px-3 py-1.5 outline-none focus:ring-1 focus:ring-blue-500/50 border-0 cursor-pointer ${className}`}
    >
      {options.map((opt) => (
        <option key={opt.value} value={opt.value}>
          {opt.label}
        </option>
      ))}
    </select>
  )
}
