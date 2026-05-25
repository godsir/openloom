export function resolveFileRefUrl(ref: any, _opts?: { connection?: unknown; platform?: unknown }): { url: string; mode?: string } {
  return { url: ref?.url || ref?.path || '', mode: 'inline' };
}
