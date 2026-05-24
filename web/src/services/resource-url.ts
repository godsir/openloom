export function resolveFileRefUrl(ref: any): { url: string; mode?: string } {
  return { url: ref?.url || ref?.path || '', mode: 'inline' };
}
