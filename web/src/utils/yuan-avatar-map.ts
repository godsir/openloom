/**
 * Loom agent avatar — white background, black "L" initial.
 * Returns an inline SVG data URI.
 */
export function loomAgentAvatarUrl(): string {
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="80" height="80" viewBox="0 0 80 80">
  <rect width="80" height="80" rx="20" fill="#ffffff" stroke="#d0d0d0" stroke-width="1"/>
  <text x="50%" y="54%" dominant-baseline="middle" text-anchor="middle"
        font-family="system-ui, -apple-system, sans-serif" font-size="36" font-weight="600" fill="#1a1a1a">L</text>
</svg>`;
  return `data:image/svg+xml,${encodeURIComponent(svg)}`;
}

export function yuanAvatarUrl(_filename: string): string {
  return loomAgentAvatarUrl();
}

export function yuanFallbackAvatarUrl(_yuan?: string): string {
  return loomAgentAvatarUrl();
}
