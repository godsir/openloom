// Formatting utilities.

export function escapeHtml(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
}

export function formatSessionDate(dateStr: string): string {
  const date = new Date(dateStr)
  const now = new Date()
  const diff = now.getTime() - date.getTime()
  const minutes = Math.floor(diff / 60000)
  const hours = Math.floor(diff / 3600000)
  const days = Math.floor(diff / 86400000)

  if (minutes < 1) return '刚刚'
  if (minutes < 60) return `${minutes} 分钟前`
  if (hours < 24) return `${hours} 小时前`
  if (days < 7) return `${days} 天前`
  return date.toLocaleDateString('zh-CN')
}

export function toSlash(text: string): string {
  return text.replace(/\\/g, '/')
}

export function baseName(path: string): string {
  return path.split(/[/\\]/).pop() || path
}

export function parseCSV(text: string): string[][] {
  return text.split('\n').filter(Boolean).map((line) =>
    line.split(',').map((cell) => cell.trim().replace(/^"|"$/g, '')),
  )
}
