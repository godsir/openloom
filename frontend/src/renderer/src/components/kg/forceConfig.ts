export const FORCE_CONFIG = {
  node: {
    minRadius: 2,
    collisionMultiplier: 20,
  },
  link: {
    normal: { sameSession: 2800, crossSession: 4800 },
    fullscreen: { sameSession: 4800, crossSession: 8000 },
    strength: 3.0,
  },
  charge: {
    strength: -1200,
    distanceMin: 80,
    distanceMax: 4000,
  },
  simulation: {
    cooldownTicks: 800,
    alphaDecay: 0.001,
    velocityDecay: 0.4,
  },
  zoom: {
    normal: { min: 0.2, max: 5 },
    fullscreen: { min: 0.1, max: 8 },
  },
}

export const THEME_COLORS_ALWAYS_LIGHT = {
  bg: '#0B0F14',
  text: 'rgba(255,255,255,0.88)',
  textSecondary: 'rgba(255,255,255,0.70)',
  textMuted: 'rgba(255,255,255,0.35)',
  border: 'rgba(255,255,255,0.08)',
}
