import { loomRpc } from './jsonrpc'
import { t } from '../i18n'

/** RPC call with automatic success/error toast feedback. */
export async function rpc<T = unknown>(
  method: string,
  params: Record<string, unknown> | undefined,
  okMsg: string,
  errLabel?: string,
): Promise<T> {
  try {
    const result = await loomRpc<T>(method, params)
    const { useStore } = await import('../stores')
    useStore.getState().addToast({ type: 'success', message: okMsg })
    return result
  } catch (e: any) {
    const { useStore } = await import('../stores')
    const label = errLabel || method
    useStore.getState().addToast({ type: 'error', message: t('error.rpcFailed', { method: label, reason: e.message || String(e) }) })
    throw e
  }
}
