import { ipcMain } from 'electron'
import { getMainWindow } from '../window'
import type { IMGatewayManager } from '../im/imGatewayManager'
import type { IMStore } from '../im/imStore'
import type { Platform, InstanceConfig } from '../im/types'

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

  // ── Channel lifecycle ──

  ipcMain.handle('im:start-channel', async (_e, platform: Platform, instanceId: string) => {
    const config = imStore.listInstances().find(
      (i) => i.platform === platform && i.instanceId === instanceId
    )
    if (config) {
      await imGatewayManager.startChannel(config)
      return { ok: true }
    }
    return { ok: false, error: 'Config not found' }
  })

  ipcMain.handle('im:stop-channel', async (_e, platform: Platform, instanceId: string) => {
    await imGatewayManager.stopChannel(platform, instanceId)
    return { ok: true }
  })

  // ── Status & connectivity ──

  ipcMain.handle('im:get-status', () => imGatewayManager.getStatus())

  ipcMain.handle('im:test-connectivity', async (_e, platform: Platform, instanceId: string) => {
    // For Electron-managed platforms (wechat), check channel status
    // For Rust-managed platforms, delegate to backend RPC
    if (platform === 'wechat' || platform === 'popo') {
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
    // Delegate to Rust backend for Telegram/Feishu/etc
    return { platform, testedAt: Date.now(), verdict: 'warn', checks: [] }
  })

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
