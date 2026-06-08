import { loomRpc } from './jsonrpc'

export interface FimCompletionResult {
  ok: boolean
  completion?: string
  message?: string
}

export async function requestFimCompletion(
  prefix: string,
  suffix: string,
  maxTokens: number = 64
): Promise<FimCompletionResult> {
  try {
    const result = await loomRpc<FimCompletionResult>('completion.fim', {
      prefix,
      suffix,
      max_tokens: maxTokens,
      api_key: '',
      base_url: 'https://api.deepseek.com/beta',
    })
    return result
  } catch {
    return { ok: false, message: 'FIM request failed' }
  }
}
