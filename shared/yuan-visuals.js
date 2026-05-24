const FALLBACK_YUAN = "hanako";

export const YUAN_VISUALS = Object.freeze({
  hanako: Object.freeze({
    yuan: "hanako",
    symbol: "✿",
    moodLabel: "温和",
    accent: "#537D96",
    avatar: "Hanako.png",
  }),
  butter: Object.freeze({
    yuan: "butter",
    symbol: "❊",
    moodLabel: "热情",
    accent: "#5BA88C",
    avatar: "Butter.png",
  }),
  ming: Object.freeze({
    yuan: "ming",
    symbol: "◈",
    moodLabel: "冷静",
    accent: "#8BA4B4",
    avatar: "Ming.png",
  }),
});

const YUAN_ACCENTS = Object.freeze({
  hanako: "#537D96",
  butter: "#5BA88C",
  ming: "#8BA4B4",
  kong: "#555555",
});

const YUAN_SYMBOLS = Object.freeze({
  hanako: "✿",
  butter: "❊",
  ming: "◈",
  kong: "○",
});

export function normalizeYuan(yuan) {
  const key = String(yuan || "").trim().toLowerCase();
  return Object.prototype.hasOwnProperty.call(YUAN_VISUALS, key) ? key : FALLBACK_YUAN;
}

export function getYuanVisual(yuan) {
  return YUAN_VISUALS[normalizeYuan(yuan)];
}

export function getYuanAccent(yuan) {
  return YUAN_ACCENTS[normalizeYuan(yuan)] || YUAN_ACCENTS.hanako;
}

export function getYuanSymbol(yuan) {
  return YUAN_SYMBOLS[normalizeYuan(yuan)] || YUAN_SYMBOLS.hanako;
}

export function moodLabelForYuan(yuan) {
  const visual = getYuanVisual(yuan);
  return `${visual.symbol} ${visual.moodLabel}`;
}
