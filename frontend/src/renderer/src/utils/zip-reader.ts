import { unzipSync } from 'fflate'

export function readZipEntries(buffer: ArrayBuffer): { path: string; content: string }[] {
  const data = new Uint8Array(buffer)
  const unzipped = unzipSync(data)
  const decoder = new TextDecoder()
  const files: { path: string; content: string }[] = []

  for (const [path, bytes] of Object.entries(unzipped)) {
    // Skip directories (end with /) and hidden files
    if (path.endsWith('/') || path.startsWith('__MACOSX')) continue
    // Strip top-level folder prefix if all entries share one
    const content = decoder.decode(bytes)
    files.push({ path, content })
  }

  // Strip common prefix (top folder)
  if (files.length > 0) {
    const firstSlash = files[0].path.indexOf('/')
    if (firstSlash > 0) {
      const prefix = files[0].path.slice(0, firstSlash + 1)
      const allSharePrefix = files.every(f => f.path.startsWith(prefix))
      if (allSharePrefix) {
        for (const f of files) {
          f.path = f.path.slice(prefix.length)
        }
      }
    }
  }

  return files.filter(f => f.path.length > 0)
}
