import React from 'react'

/**
 * Theme-adaptive SVG banner for openLoom welcome screen.
 * All colors come from CSS variables — auto-adapts to every theme.
 */
export const BannerLogo: React.FC<{ className?: string }> = ({ className }) => {
  return (
    <svg
      className={className}
      viewBox="0 0 480 180"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      style={{ width: '100%', height: 'auto', display: 'block' }}
    >
      <defs>
        {/* Accent glow gradient */}
        <radialGradient id="bgGlow1" cx="22%" cy="30%" r="55%">
          <stop offset="0%" stopColor="var(--accent, #22D3EE)" stopOpacity="0.12" />
          <stop offset="100%" stopColor="var(--accent, #22D3EE)" stopOpacity="0" />
        </radialGradient>
        <radialGradient id="bgGlow2" cx="78%" cy="65%" r="50%">
          <stop offset="0%" stopColor="var(--accent, #22D3EE)" stopOpacity="0.08" />
          <stop offset="100%" stopColor="var(--accent, #22D3EE)" stopOpacity="0" />
        </radialGradient>
        {/* Title gradient: accent → text */}
        <linearGradient id="titleGrad" x1="0" y1="0" x2="1" y2="1">
          <stop offset="0%" stopColor="var(--accent, #22D3EE)" />
          <stop offset="60%" stopColor="var(--accent, #22D3EE)" stopOpacity="0.7" />
          <stop offset="100%" stopColor="var(--text, #fff)" stopOpacity="0.85" />
        </linearGradient>
        {/* Subtitle gradient */}
        <linearGradient id="subGrad" x1="0" y1="0" x2="1" y2="0">
          <stop offset="0%" stopColor="var(--text, #fff)" stopOpacity="0.45" />
          <stop offset="50%" stopColor="var(--text, #fff)" stopOpacity="0.6" />
          <stop offset="100%" stopColor="var(--text, #fff)" stopOpacity="0.45" />
        </linearGradient>
      </defs>

      {/* Background glow circles */}
      <circle cx="105" cy="60" r="130" fill="url(#bgGlow1)" />
      <circle cx="375" cy="120" r="120" fill="url(#bgGlow2)" />

      {/* Decorative grid lines — left */}
      <g stroke="var(--border, rgba(255,255,255,0.06))" strokeWidth="0.5">
        <line x1="30" y1="30" x2="110" y2="30" />
        <line x1="30" y1="42" x2="90" y2="42" />
        <line x1="30" y1="54" x2="70" y2="54" />
        <line x1="60" y1="18" x2="60" y2="54" />
        <line x1="80" y1="18" x2="80" y2="42" />
      </g>

      {/* Decorative grid lines — right */}
      <g stroke="var(--border, rgba(255,255,255,0.06))" strokeWidth="0.5">
        <line x1="370" y1="140" x2="450" y2="140" />
        <line x1="390" y1="150" x2="450" y2="150" />
        <line x1="340" y1="128" x2="340" y2="155" />
        <line x1="360" y1="128" x2="360" y2="148" />
      </g>

      {/* Accent dot decorations */}
      <circle cx="55" cy="115" r="1.5" fill="var(--accent, #22D3EE)" opacity="0.3" />
      <circle cx="420" cy="45" r="1.5" fill="var(--accent, #22D3EE)" opacity="0.3" />
      <circle cx="400" cy="65" r="1" fill="var(--accent, #22D3EE)" opacity="0.2" />
      <circle cx="85" cy="140" r="1" fill="var(--accent, #22D3EE)" opacity="0.2" />

      {/* ── Logo: "LOOM" with infinity-loop OO ── */}
      <g transform="translate(240, 88)">
        {/* L */}
        <path
          d="M-115 -36 v72 h30"
          stroke="url(#titleGrad)"
          strokeWidth="7"
          strokeLinecap="round"
          strokeLinejoin="round"
          fill="none"
        />
        {/* First O (left half of infinity) */}
        <path
          d="M-65 -6 a26 26 0 1 1 0 12"
          stroke="url(#titleGrad)"
          strokeWidth="7"
          strokeLinecap="round"
          fill="none"
        />
        {/* Second O (right half of infinity)  */}
        <path
          d="M-13 -6 a26 26 0 1 1 0 12"
          stroke="url(#titleGrad)"
          strokeWidth="7"
          strokeLinecap="round"
          fill="none"
        />
        {/* Infinity bridge between the two O's */}
        <path
          d="M-39 6 q13 -8 26 -8 q13 0 26 8"
          stroke="var(--accent, #22D3EE)"
          strokeWidth="3"
          strokeLinecap="round"
          fill="none"
          opacity="0.6"
        />
        {/* M */}
        <path
          d="M40 -36 v72 l18 -28 l18 28 v-72"
          stroke="url(#titleGrad)"
          strokeWidth="7"
          strokeLinecap="round"
          strokeLinejoin="round"
          fill="none"
        />
      </g>

      {/* Subtitle line */}
      <text
        x="240"
        y="154"
        textAnchor="middle"
        fill="url(#subGrad)"
        fontFamily="var(--font, Inter, sans-serif)"
        fontSize="11"
        fontWeight="400"
        letterSpacing="3"
      >
        PRIVATE AI ASSISTANT
      </text>
    </svg>
  )
}

export default BannerLogo
