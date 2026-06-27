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
    // WeChat
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
    // Telegram
    if (platform === 'telegram') {
      const ch = imGatewayManager.channels?.get(`${platform}:${instanceId}`)
      const isConnected = ch?.isConnected ?? false
      return {
        platform,
        testedAt: Date.now(),
        verdict: isConnected ? 'pass' : 'warn',
        checks: [{
          code: 'gateway_running',
          level: isConnected ? 'pass' : 'warn',
          message: isConnected
            ? `Channel connected${ch?.currentAccountId ? ` (${ch.currentAccountId})` : ''}`
            : 'Channel not connected',
        }],
      }
    }
    // Discord
    if (platform === 'discord') {
      const ch = imGatewayManager.channels?.get(`${platform}:${instanceId}`)
      const isConnected = ch?.isConnected ?? false
      return {
        platform,
        testedAt: Date.now(),
        verdict: isConnected ? 'pass' : 'warn',
        checks: [{
          code: 'gateway_running',
          level: isConnected ? 'pass' : 'warn',
          message: isConnected
            ? `Channel connected${ch?.currentAccountId ? ` (${ch.currentAccountId})` : ''}`
            : 'Channel not connected',
        }],
      }
    }
    // QQ
    if (platform === 'qq') {
      const ch = imGatewayManager.channels?.get(`${platform}:${instanceId}`)
      const isConnected = ch?.isConnected ?? false
      return {
        platform,
        testedAt: Date.now(),
        verdict: isConnected ? 'pass' : 'warn',
        checks: [{
          code: 'gateway_running',
          level: isConnected ? 'pass' : 'warn',
          message: isConnected
            ? `Channel connected${ch?.currentAccountId ? ` (${ch.currentAccountId})` : ''}`
            : 'Channel not connected',
        }],
      }
    }
    // 飞书
    if (platform === 'feishu') {
      const ch = imGatewayManager.channels?.get(`${platform}:${instanceId}`)
      const isConnected = ch?.isConnected ?? false
      return {
        platform,
        testedAt: Date.now(),
        verdict: isConnected ? 'pass' : 'warn',
        checks: [{
          code: 'gateway_running',
          level: isConnected ? 'pass' : 'warn',
          message: isConnected
            ? `Channel connected${ch?.currentAccountId ? ` (${ch.currentAccountId})` : ''}`
            : 'Channel not connected',
        }],
      }
    }
    // 企业微信
    if (platform === 'wecom') {
      const ch = imGatewayManager.channels?.get(`${platform}:${instanceId}`)
      const isConnected = ch?.isConnected ?? false
      return {
        platform,
        testedAt: Date.now(),
        verdict: isConnected ? 'pass' : 'warn',
        checks: [{
          code: 'gateway_running',
          level: isConnected ? 'pass' : 'warn',
          message: isConnected
            ? `Channel connected${ch?.currentAccountId ? ` (${ch.currentAccountId})` : ''}`
            : 'Channel not connected',
        }],
      }
    }
    // 钉钉
    if (platform === 'dingtalk') {
      const ch = imGatewayManager.channels?.get(`${platform}:${instanceId}`)
      const isConnected = ch?.isConnected ?? false
      return {
        platform,
        testedAt: Date.now(),
        verdict: isConnected ? 'pass' : 'warn',
        checks: [{
          code: 'gateway_running',
          level: isConnected ? 'pass' : 'warn',
          message: isConnected
            ? `Channel connected${ch?.currentAccountId ? ` (${ch.currentAccountId})` : ''}`
            : 'Channel not connected',
        }],
      }
    }
    // POPO
    if (platform === 'popo') {
      const ch = imGatewayManager.channels?.get(`${platform}:${instanceId}`)
      const isConnected = ch?.isConnected ?? false
      return {
        platform,
        testedAt: Date.now(),
        verdict: isConnected ? 'pass' : 'warn',
        checks: [{
          code: 'gateway_running',
          level: isConnected ? 'pass' : 'warn',
          message: isConnected
            ? `Channel connected${ch?.currentAccountId ? ` (${ch.currentAccountId})` : ''}`
            : 'Channel not connected',
        }],
      }
    }
    // Unknown platform — should not reach here since all are now covered
    return {
      platform,
      testedAt: Date.now(),
      verdict: 'fail',
      checks: [{
        code: 'not_implemented',
        level: 'fail',
        message: '尚未实现',
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

  ipcMain.handle('im:popo-qr-start', async (_e, instanceId: string) => {
    return imGatewayManager.popoQrStart(instanceId)
  })

  ipcMain.handle('im:popo-qr-poll', async (_e, taskToken: string) => {
    return imGatewayManager.popoQrPoll(taskToken)
  })

  // ── Telegram Token 登录 ──

  ipcMain.handle('im:telegram-login', async (_e, platform: Platform, instanceId: string, token: string) => {
    try {
      return await imGatewayManager.telegramLogin(platform, instanceId, token);
    } catch (err: any) {
      return { ok: false, error: err?.message || String(err) };
    }
  });

  // ── Discord Token 登录 ──

  ipcMain.handle('im:discord-login', async (_e, platform: Platform, instanceId: string, token: string) => {
    try {
      return await imGatewayManager.discordLogin(platform, instanceId, token);
    } catch (err: any) {
      return { ok: false, error: err?.message || String(err) };
    }
  });

  // ── QQ OAuth 登录 ──

  ipcMain.handle('im:qq-login', async (_e, platform: Platform, instanceId: string, appId: string, clientSecret: string) => {
    try {
      return await imGatewayManager.qqLogin(platform, instanceId, appId, clientSecret);
    } catch (err: any) {
      return { ok: false, error: err?.message || String(err) };
    }
  });

  // ── 飞书 应用登录 ──

  ipcMain.handle('im:feishu-login', async (_e, platform: Platform, instanceId: string, appId: string, appSecret: string) => {
    try {
      return await imGatewayManager.feishuLogin(platform, instanceId, appId, appSecret);
    } catch (err: any) {
      return { ok: false, error: err?.message || String(err) };
    }
  });

  // ── 企业微信 应用登录 ──

  ipcMain.handle('im:wecom-login', async (_e, platform: Platform, instanceId: string, corpId: string, secret: string, agentId: string) => {
    try {
      return await imGatewayManager.wecomLogin(platform, instanceId, corpId, secret, agentId);
    } catch (err: any) {
      return { ok: false, error: err?.message || String(err) };
    }
  });

  // ── 钉钉 应用登录 ──

  ipcMain.handle('im:dingtalk-login', async (_e, platform: Platform, instanceId: string, appKey: string, appSecret: string) => {
    try {
      return await imGatewayManager.dingtalkLogin(platform, instanceId, appKey, appSecret);
    } catch (err: any) {
      return { ok: false, error: err?.message || String(err) };
    }
  });

  // ── Forward channel status events to renderer ──

  imGatewayManager.on('channel-status', (status) => {
    const mainWindow = getMainWindow()
    mainWindow?.webContents.send('im:channel-status', status)
  })
}
