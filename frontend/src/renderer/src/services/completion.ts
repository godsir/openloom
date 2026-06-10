import { loomRpc } from './jsonrpc'

export interface FimCompletionResult {
  ok: boolean
  completion?: string
  message?: string
}

interface FimConfig {
  model: string | null
  base_url: string | null
  api_key_env: string | null
}

let cachedFimConfig: FimConfig | null = null
let cacheTime = 0
const CACHE_TTL = 30_000 // 30s cache to avoid RPC on every keystroke

async function getFimConfig(): Promise<FimConfig> {
  const now = Date.now()
  if (cachedFimConfig && now - cacheTime < CACHE_TTL) {
    return cachedFimConfig
  }
  try {
    const config = await loomRpc<FimConfig>('config.get_fim')
    cachedFimConfig = config
    cacheTime = now
    return config
  } catch {
    return { model: null, base_url: null, api_key_env: null }
  }
}

/** Invalidate the FIM config cache — call after config.set_fim to ensure immediate effect. */
export function invalidateFimCache(): void {
  cachedFimConfig = null
  cacheTime = 0
}

export async function requestFimCompletion(
  prefix: string,
  suffix: string,
  maxTokens: number = 64
): Promise<FimCompletionResult> {
  try {
    // Load the user-configured FIM model
    const config = await getFimConfig()

    const result = await loomRpc<FimCompletionResult>('completion.fim', {
      prefix,
      suffix,
      max_tokens: maxTokens,
      // Pass the configured model name — backend resolves to actual model ID + base_url + key
      model: config.model || undefined,
    })
    return result
  } catch {
    return { ok: false, message: 'FIM request failed' }
  }
}
