/**
 * i18n.ts — 加载中文 locale，挂到 window.t / window.i18n
 *
 * 从 zh.json 加载翻译，支持嵌套 key（用 . 分隔）和变量插值（{var}）。
 * 在 React 挂载之前同步初始化。
 */

import zhRaw from './zh.json';

type LocaleData = Record<string, any>;

const zh: LocaleData = zhRaw;

/**
 * 深度取值：'sidebar.title' → zh.sidebar.title
 */
function deepGet(obj: LocaleData, key: string): any {
  const parts = key.split('.');
  let cur: any = obj;
  for (const p of parts) {
    if (cur == null || typeof cur !== 'object') return undefined;
    cur = cur[p];
  }
  if (typeof cur === 'string') return cur;
  if (Array.isArray(cur) || typeof cur === 'object') return cur;
  return undefined;
}

/**
 * 变量插值：'你好 {name}' + {name: '世界'} → '你好 世界'
 */
function interpolate(str: string, vars?: Record<string, string | number>): string {
  if (!vars) return str;
  return str.replace(/\{(\w+)\}/g, (_m, k) => {
    const v = vars[k];
    return v !== undefined ? String(v) : `{${k}}`;
  });
}

/**
 * t(key, vars?) — 翻译函数
 */
export function t(key: string, vars?: Record<string, string | number>): any {
  const val = deepGet(zh, key);
  if (val === undefined) return key;
  if (typeof val === 'string') return interpolate(val, vars);
  return val;
}

/**
 * 初始化：挂到 window.t / window.i18n
 * 在 React 挂载之前调用。
 */
export function initI18n(): void {
  (window as any).t = t;
  (window as any).i18n = {
    t,
    locale: 'zh',
    load: () => Promise.resolve(),
    defaultName: 'Loom',
  };
}
