export function getPlatform(): string {
  return (window.platform as any)?.getPlatform?.() ?? 'win32';
}
export function usePlatform(): string {
  return getPlatform();
}
