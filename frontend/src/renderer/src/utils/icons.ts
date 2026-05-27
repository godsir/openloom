// SVG icon registry with file-type-to-icon mapping.
type IconFn = (size?: number) => string

const registry = new Map<string, IconFn>()

export function registerIcon(name: string, fn: IconFn): void {
  registry.set(name, fn)
}

export function getIcon(name: string, size = 16): string {
  const fn = registry.get(name)
  return fn ? fn(size) : ''
}

// File-type to icon mapping
const FILE_ICON_MAP: Record<string, string> = {
  code: 'file-code',
  text: 'file-text',
  image: 'file-image',
  doc: 'file-doc',
  spreadsheet: 'file-spreadsheet',
  video: 'file-video',
}

export function iconForKind(kind: string): string {
  return FILE_ICON_MAP[kind] || 'file'
}
