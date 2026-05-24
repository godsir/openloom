import { useEffect, useRef, useState } from 'react';
import { loomRpc } from '../adapter';

let configCache: any = null;
let cacheTime = 0;
const STALE_MS = 5000; // 5s stale window

/** Invalidate cache (call after config.set or on WS config_changed) */
export function invalidateConfigCache() {
  configCache = null;
  cacheTime = 0;
}

/** Fetch config with in-memory cache */
export async function fetchConfig(): Promise<any> {
  if (configCache && Date.now() - cacheTime < STALE_MS) return configCache;
  const data = await loomRpc('config.get');
  configCache = data;
  cacheTime = Date.now();
  return data;
}

/**
 * React hook: returns config and a refresh function.
 * Auto-fetches on mount. Use `refresh()` after mutations.
 */
export function useConfig() {
  const [config, setConfig] = useState<any>(configCache);
  const mounted = useRef(true);

  useEffect(() => {
    mounted.current = true;
    fetchConfig().then(d => { if (mounted.current) setConfig(d); }).catch(() => {});
    return () => { mounted.current = false; };
  }, []);

  const refresh = async () => {
    invalidateConfigCache();
    const d = await fetchConfig();
    if (mounted.current) setConfig(d);
    return d;
  };

  return { config, refresh };
}
