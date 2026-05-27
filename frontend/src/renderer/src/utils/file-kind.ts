// File extension to kind mapping — single source of truth for file categorization.
const EXT_TO_KIND: Record<string, string> = {
  // Code
  ts: 'code', tsx: 'code', js: 'code', jsx: 'code', py: 'code',
  rs: 'code', go: 'code', java: 'code', c: 'code', cpp: 'code',
  rb: 'code', php: 'code', swift: 'code', kt: 'code', scala: 'code',
  // Text
  txt: 'text', md: 'text', json: 'text', yaml: 'text', yml: 'text',
  toml: 'text', xml: 'text', csv: 'text', log: 'text', sql: 'text',
  // Image
  png: 'image', jpg: 'image', jpeg: 'image', gif: 'image', svg: 'image',
  webp: 'image', ico: 'image', bmp: 'image',
  // Document
  pdf: 'doc', doc: 'doc', docx: 'doc', xls: 'spreadsheet', xlsx: 'spreadsheet',
  // Video
  mp4: 'video', webm: 'video', mov: 'video', avi: 'video',
}

export function inferKindByExt(filename: string): string {
  const ext = filename.split('.').pop()?.toLowerCase() || ''
  return EXT_TO_KIND[ext] || 'unknown'
}

export function extOfName(filename: string): string {
  return filename.split('.').pop()?.toLowerCase() || ''
}

export function isImageOrSvgExt(filename: string): boolean {
  const kind = inferKindByExt(filename)
  return kind === 'image'
}

export function isMediaKind(filename: string): boolean {
  const kind = inferKindByExt(filename)
  return kind === 'image' || kind === 'video'
}
