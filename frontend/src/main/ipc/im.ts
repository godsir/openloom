import { ipcMain } from 'electron'
import { getMainWindow } from '../window'
import type { IMGatewayManager } from '../im/imGatewayManager'
import type { IMStore } from '../im/imStore'
import type { Platform, InstanceConfig, IMSettings } from '../im/types'

export function registerImIpc(): void {
  const imStore = (global as any).__imStore as IMStore | undefined
  const imGatewayManager = (global as any).__imGatewayManager as IMGatewayManager | undefined

  if (!imStore || !imGatewayManager) {
    console.warn('[IM IPC] IM store or gateway manager not initialized, skipping IPC registration')
    return
  }

  // ── Config CRUD ──

  ipcMain.handle('im:list-configs', () => imStore.listInstances())

  ipcMain.handle('im:set-config', (_e, config: InstanceConfig) => {
    imStore.upsertInstance(config)
    return { ok: true }
  })

  ipcMain.handle('im:delete-config', (_e, platform: Platform, instanceId: string) => {
    imStore.deleteInstance(platform, instanceId)
    return { ok: true }
  })

  // ── Global settings ──

  ipcMain.handle('im:get-settings', () => imStore.getSettings())

  ipcMain.handle('im:set-settings', (_e, settings: Partial<IMSettings>) => {
    imStore.setSettings(settings)
    return { ok: true }
  })

  // ── Channel lifecycle ──

  ipcMain.handle('im:start-channel', async (_e, platform: Platform, instanceId: string) => {
    const config = imStore.listInstances().find(
      (i) => i.platform === platform && i.instanceId === instanceId
    )
    if (!config) return { ok: false, error: 'Config not found' }
    try {
      await imGatewayManager.startChannel(config)
      return { ok: true }
    } catch (err: any) {
      return { ok: false, error: err?.message || String(err) }
    }
  })

  ipcMain.handle('im:stop-channel', async (_e, platform: Platform, instanceId: string) => {
    try {
      await imGatewayManager.stopChannel(platform, instanceId)
      return { ok: true }
    } catch (err: any) {
      return { ok: false, error: err?.message || String(err) }
    }
  })

  // ── Status & connectivity ──

  ipcMain.handle('im:get-status', () => imGatewayManager.getStatus())

  ipcMain.handle('im:test-connectivity', async (_e, platform: Platform, instanceId: string) => {
    // Only WeChat is currently implemented in the Electron layer.
    if (platform === 'wechat') {
      const ch = imGatewayManager.channels?.get(`${platform}:${instanceId}`)
      return {
        platform,
        testedAt: Date.now(),
        verdict: ch?.isConnected ? 'pass' : 'warn',
        checks: [{
          code: 'gateway_running',
          level: ch?.isConnected ? 'pass' : 'warn',
          message: ch?.isConnected ? 'Channel connected' : 'Channel not connected',
        }],
      }
    }
    // Other platforms are not yet implemented in the backend.
    return {
      platform,
      testedAt: Date.now(),
      verdict: 'fail',
      checks: [{
        code: 'not_implemented',
        level: 'fail',
        message: `${platform} 接入尚未实现`,
        suggestion: '当前仅支持微信扫码接入，其他平台即将支持',
      }],
    }
  })

  // ── Send help message (test connection) ──

  ipcMain.handle('im:send-help', async (_e, platform: Platform, instanceId: string) => {
    try {
      return await imGatewayManager.sendHelpMessage(platform, instanceId);
    } catch (e: any) {
      return { ok: false, error: e?.message || String(e) };
    }
  });

  // ── Session bindings (for detecting IM sessions in the desktop sidebar) ──

  ipcMain.handle('im:list-session-bindings', () => {
    return imStore.listAllBindings();
  });

  // ── WeChat QR flow ──

  ipcMain.handle('im:wechat-qr-start', async (_e, instanceId: string) => {
    return imGatewayManager.wechatQrStart(instanceId)
  })

  ipcMain.handle('im:wechat-qr-wait', async (_e, instanceId: string, sessionKey: string) => {
    return imGatewayManager.wechatQrWait(instanceId, sessionKey)
  })

  // ── POPO QR flow ──

  ipcMain.handle('im:popo-qr-start', () => imGatewayManager.popoQrStart())

  ipcMain.handle('im:popo-qr-poll', async (_e, taskToken: string) => {
    return imGatewayManager.popoQrPoll(taskToken)
  })

  // ── Forward channel status events to renderer ──

  imGatewayManager.on('channel-status', (status) => {
    const mainWindow = getMainWindow()
    mainWindow?.webContents.send('im:channel-status', status)
  })
}
