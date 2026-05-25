/**
 * bootstrap.ts — 主题初始化 + 拖拽阻止 + i18n
 *
 * 主题通过 html[data-theme="xxx"] 激活，所有主题 CSS 已在 main.tsx 全部 import。
 * 运行时切换只需改 data-theme 属性即可，无需动态插 <link>。
 */

import zhDict from './lib/zh.json';

// ── i18n ─────────────────────────────────────────────────────────────────

/**
 * 点路径查找：'input.operateMode' → dict.input.operateMode
 */
function lookup(dict: Record<string, any>, key: string): string | undefined {
  if (!key) return undefined;
  if (Object.prototype.hasOwnProperty.call(dict, key)) return dict[key];
  const parts = key.split('.');
  let cur: any = dict;
  for (const part of parts) {
    if (cur == null || typeof cur !== 'object') return undefined;
    cur = cur[part];
  }
  return typeof cur === 'string' ? cur : undefined;
}

function interpolate(str: string, vars?: Record<string, string | number>): string {
  if (!vars) return str;
  return str.replace(/\{(\w+)\}/g, (_, k) => (k in vars ? String(vars[k]) : `{${k}}`));
}

function makeT(dict: Record<string, any>) {
  return function t(key: string, vars?: Record<string, string | number>): string {
    const raw = lookup(dict, key);
    if (raw == null) return key;
    return interpolate(raw, vars);
  };
}

const _t = makeT(zhDict as Record<string, any>);
(window as any).t = _t;
(window as any).i18n = {
  t: _t,
  locale: 'zh',
  load: () => Promise.resolve(),
  defaultName: (window as any).i18n?.defaultName || 'Loom',
};

const THEME_STORAGE_KEY = 'hana-theme';

/** 切换主题：更新 html data-theme 属性 + 持久化 */
export function applyTheme(themeId: string): void {
  document.documentElement.setAttribute('data-theme', themeId);
}

/** 读 localStorage 保存的主题，默认 new-warm-paper（index.html 的内联脚本已执行一次，这里补 window 方法） */
export function initTheme(): void {
  // index.html 内联脚本已经在 React 挂载前设置了 data-theme，这里只补挂 window 方法
  const saved = localStorage.getItem(THEME_STORAGE_KEY) || 'new-warm-paper';
  applyTheme(saved);

  // 挂到 window 供 Hanako 组件调用（如 use-theme.ts、设置面板等）
  (window as any).setTheme = (id: string) => {
    applyTheme(id);
    try { localStorage.setItem(THEME_STORAGE_KEY, id); } catch {}
  };
  (window as any).loadSavedTheme = () => {
    const t = localStorage.getItem(THEME_STORAGE_KEY) || 'new-warm-paper';
    applyTheme(t);
  };

  // Font stub（Hanako 有些地方会调用，Loom 暂不支持）
  (window as any).setSerifFont = (_font: string) => {};
  (window as any).loadSavedFont = () => {};
}

/** 阻止拖拽文件时浏览器跳转 */
export function initDragPrevention(): void {
  document.addEventListener('dragover', e => e.preventDefault());
  document.addEventListener('drop', e => e.preventDefault());
}
