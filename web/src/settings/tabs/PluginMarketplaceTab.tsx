import React, { useCallback, useEffect, useState } from 'react';
import { useSettingsStore } from '../store';
import { hanaFetch } from '../api';
import { t } from '../helpers';
import { SettingsSection } from '../components/SettingsSection';
import { renderMarkdown } from '../../utils/markdown';
import styles from '../Settings.module.css';

interface MarketplacePlugin {
  id: string;
  name: string;
  publisher?: string;
  version?: string;
  description?: string;
  trust?: 'restricted' | 'full-access';
  permissions?: string[];
  contributions?: string[];
  repository?: string | null;
  compatibility?: { minAppVersion?: string; hanaApi?: string };
  distribution?: { kind?: 'source' | 'release'; path?: string; packageUrl?: string; sha256?: string } | null;
  installed?: boolean;
  installedVersion?: string | null;
  latestVersion?: string | null;
  selectedVersion?: string | null;
  updateAvailable?: boolean;
  downgrade?: boolean;
  reinstall?: boolean;
  compatible?: boolean;
  canInstall?: boolean;
  installAction?: 'install' | 'update' | 'downgrade' | 'reinstall' | 'incompatible';
}

interface MarketplaceResponse {
  source?: { kind?: string; configured?: boolean; path?: string; url?: string };
  plugins: MarketplacePlugin[];
  warnings?: string[];
}

interface MarketplaceSource {
  kind: string;
  name: string;
  url?: string | null;
  path?: string | null;
  configured: boolean;
}

function marketVersion(plugin: MarketplacePlugin): string {
  return plugin.selectedVersion || plugin.latestVersion || plugin.version || '0.0.0';
}

function marketInstallLabel(plugin: MarketplacePlugin): string {
  if (plugin.compatible === false || plugin.installAction === 'incompatible') return t('settings.plugins.marketIncompatible');
  if (plugin.installAction === 'downgrade') return t('settings.plugins.marketDowngrade');
  if (plugin.installAction === 'reinstall') return t('settings.plugins.marketReinstall');
  if (plugin.installAction === 'update' || plugin.updateAvailable) return t('settings.plugins.marketUpdate');
  return t('settings.plugins.marketInstall');
}

function marketVersionStatus(plugin: MarketplacePlugin): string | null {
  if (plugin.compatible === false || plugin.installAction === 'incompatible') return t('settings.plugins.marketIncompatible');
  if (plugin.installAction === 'downgrade') {
    return t('settings.plugins.marketDowngradeTo', { version: marketVersion(plugin) });
  }
  if (plugin.updateAvailable && plugin.installedVersion) {
    return t('settings.plugins.marketUpdateFrom', {
      from: plugin.installedVersion,
      to: marketVersion(plugin),
    });
  }
  if (plugin.installedVersion) return t('settings.plugins.marketInstalledVersion', { version: plugin.installedVersion });
  return null;
}

export function PluginMarketplaceTab() {
  const showToast = useSettingsStore(s => s.showToast);
  const set = useSettingsStore(s => s.set);
  const [marketplace, setMarketplace] = useState<MarketplaceResponse | null>(null);
  const [marketplaceLoading, setMarketplaceLoading] = useState(false);
  const [selectedPlugin, setSelectedPlugin] = useState<MarketplacePlugin | null>(null);
  const [readme, setReadme] = useState('');
  const [readmeLoading, setReadmeLoading] = useState(false);
  const [installingPluginId, setInstallingPluginId] = useState<string | null>(null);
  const [showSources, setShowSources] = useState(false);
  const [sources, setSources] = useState<MarketplaceSource[]>([]);
  const [addUrl, setAddUrl] = useState('');
  const [addName, setAddName] = useState('');
  const [sourcesLoading, setSourcesLoading] = useState(false);
  const [translatingPluginId, setTranslatingPluginId] = useState<string | null>(null);
  const [translatedReadme, setTranslatedReadme] = useState('');

  const loadSources = useCallback(async () => {
    setSourcesLoading(true);
    try {
      const res = await hanaFetch('/api/plugins/marketplace/sources');
      const data = await res.json();
      setSources(Array.isArray(data.sources) ? data.sources : []);
    } catch { /* ignore */ }
    finally { setSourcesLoading(false); }
  }, []);

  const addSource = async () => {
    const url = addUrl.trim();
    const name = addName.trim() || url.split('/').pop()?.replace('.git', '') || 'Marketplace';
    if (!url) return;
    try {
      const res = await hanaFetch('/api/plugins/marketplace/sources', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ url, name }),
      });
      const data = await res.json();
      if (data.error) throw new Error(data.error);
      showToast(`已添加市场源: ${name}`, 'success');
      setAddUrl('');
      setAddName('');
      await loadSources();
      await loadMarketplace();
    } catch (err: unknown) {
      showToast('添加失败: ' + (err instanceof Error ? err.message : String(err)), 'error');
    }
  };

  const removeSource = async (name: string) => {
    try {
      const res = await hanaFetch('/api/plugins/marketplace/sources', {
        method: 'DELETE',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ name }),
      });
      const data = await res.json();
      if (data.error) throw new Error(data.error);
      showToast(`已移除: ${name}`, 'success');
      await loadSources();
      await loadMarketplace();
    } catch (err: unknown) {
      showToast('移除失败: ' + (err instanceof Error ? err.message : String(err)), 'error');
    }
  };

  const refreshSources = async () => {
    try {
      await hanaFetch('/api/plugins/marketplace/refresh', { method: 'POST' });
      showToast('已刷新', 'success');
      await loadMarketplace();
    } catch (err: unknown) {
      showToast('刷新失败: ' + (err instanceof Error ? err.message : String(err)), 'error');
    }
  };

  const loadReadme = useCallback(async (plugin: MarketplacePlugin) => {
    setSelectedPlugin(plugin);
    setReadme('');
    setReadmeLoading(true);
    try {
      const res = await hanaFetch(`/api/plugins/marketplace/${encodeURIComponent(plugin.id)}/readme`);
      const data = await res.json();
      if (data.error) throw new Error(data.error);
      setReadme(data.markdown || '');
    } catch (err: unknown) {
      showToast(t('settings.plugins.marketReadmeLoadError') + ': ' + (err instanceof Error ? err.message : String(err)), 'error');
    } finally {
      setReadmeLoading(false);
    }
  }, [showToast]);

  const loadMarketplace = useCallback(async () => {
    setMarketplaceLoading(true);
    try {
      const res = await hanaFetch('/api/plugins/marketplace');
      const data = await res.json();
      if (data.error) throw new Error(data.error);
      const plugins = Array.isArray(data.plugins) ? data.plugins : [];
      const next = {
        source: data.source || {},
        plugins,
        warnings: Array.isArray(data.warnings) ? data.warnings : [],
      };
      setMarketplace(next);
      if (plugins.length > 0) {
        await loadReadme(plugins[0]);
      } else {
        setSelectedPlugin(null);
        setReadme('');
      }
    } catch (err: unknown) {
      showToast(t('settings.plugins.marketLoadError') + ': ' + (err instanceof Error ? err.message : String(err)), 'error');
    } finally {
      setMarketplaceLoading(false);
    }
  }, [loadReadme, showToast]);

  useEffect(() => {
    loadMarketplace();
  }, [loadMarketplace]);

  const installPlugin = async (plugin: MarketplacePlugin) => {
    const allowDowngrade = plugin.installAction === 'downgrade'
      ? window.confirm(t('settings.plugins.marketDowngradeConfirm', {
          from: plugin.installedVersion || '',
          to: marketVersion(plugin),
        }))
      : false;
    if (plugin.installAction === 'downgrade' && !allowDowngrade) return;

    setInstallingPluginId(plugin.id);
    try {
      const res = await hanaFetch(`/api/plugins/marketplace/${encodeURIComponent(plugin.id)}/install`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          version: plugin.selectedVersion || undefined,
          allowDowngrade,
        }),
      });
      const data = await res.json();
      if (data.error) throw new Error(data.error);
      showToast(t('settings.plugins.installSuccess', { name: data.name || plugin.name }), 'success');
      await loadMarketplace();
    } catch (err: unknown) {
      showToast(t('settings.plugins.installError') + ': ' + (err instanceof Error ? err.message : String(err)), 'error');
    } finally {
      setInstallingPluginId(null);
    }
  };

  const translateReadme = async (plugin: MarketplacePlugin) => {
    const sourceText = readme || plugin.description || '';
    if (!sourceText.trim()) {
      showToast(t('settings.plugins.marketReadmeLoadError'), 'error');
      return;
    }
    setTranslatingPluginId(plugin.id);
    setTranslatedReadme('');
    try {
      const res = await hanaFetch('/api/translate', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ text: sourceText, target: 'Simplified Chinese (zh-CN)' }),
      });
      const data = await res.json();
      if (data.error) throw new Error(data.error);
      setTranslatedReadme(data.translated || '');
    } catch (err: unknown) {
      showToast(t('settings.plugins.installError') + ': ' + (err instanceof Error ? err.message : String(err)), 'error');
    } finally {
      setTranslatingPluginId(null);
    }
  };

  // Clear translation when selecting a different plugin
  const selectPlugin = (plugin: MarketplacePlugin) => {
    if (selectedPlugin?.id !== plugin.id) {
      setTranslatedReadme('');
    }
    setSelectedPlugin(plugin);
    loadReadme(plugin);
  };

  const statusText = marketplace?.source?.configured
    ? t('settings.plugins.marketplaceCount', { count: String(marketplace.plugins.length) })
    : t('settings.plugins.marketplaceNoSource');

  return (
    <div className={`${styles['settings-tab-content']} ${styles['active']}`} data-tab="plugin-marketplace">
      <div className={styles['plugin-marketplace-toolbar']}>
        <button
          type="button"
          className={styles['settings-return-btn']}
          onClick={() => set({ activeTab: 'plugins' })}
          aria-label={t('settings.plugins.marketBack')}
          title={t('settings.plugins.marketBack')}
        >
          <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round">
            <path d="M15 18l-6-6 6-6" />
          </svg>
        </button>
        <span className={styles['skills-list-desc']}>{t('settings.plugins.marketplaceHint')}</span>
        <div className={styles['plugin-marketplace-toolbar-actions']}>
          {marketplace && (
            <span className={styles['skills-source-badge']} style={{ marginRight: 0 }}>
              {statusText}
            </span>
          )}
          <button
            type="button"
            className={styles['settings-icon-btn']}
            title={t('settings.plugins.openMarketplace')}
            onClick={loadMarketplace}
            disabled={marketplaceLoading}
          >
            <svg
              width="14" height="14" viewBox="0 0 24 24" fill="none"
              stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"
              className={marketplaceLoading ? styles['spin'] : ''}
            >
              <polyline points="23 4 23 10 17 10" />
              <path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10" />
            </svg>
          </button>
          <button
            type="button"
            className={styles['settings-icon-btn']}
            title="配置市场源"
            onClick={() => { setShowSources(!showSources); if (!showSources) loadSources(); }}
          >
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
              <circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"/>
            </svg>
          </button>
        </div>
      </div>

      {showSources && (
        <div style={{
          background: 'var(--bg-card, var(--bg-secondary))',
          border: '1px solid var(--border)',
          borderRadius: 'var(--radius)',
          padding: 'var(--space-md)',
          marginBottom: 'var(--space-md)',
        }}>
          <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 'var(--space-sm)' }}>
            <span style={{ fontWeight: 600, fontSize: '0.85rem' }}>市场源</span>
            <div style={{ display: 'flex', gap: 'var(--space-xs)' }}>
              <button
                type="button"
                className={styles['settings-save-btn-sm']}
                onClick={refreshSources}
                style={{ fontSize: '0.7rem', padding: '3px 10px' }}
              >
                刷新全部
              </button>
              <button
                type="button"
                className={styles['settings-save-btn-sm']}
                onClick={() => setShowSources(false)}
                style={{ fontSize: '0.7rem', padding: '3px 10px', background: 'var(--bg-hover)', color: 'var(--text-muted)' }}
              >
                关闭
              </button>
            </div>
          </div>

          {sources.length === 0 && !sourcesLoading && (
            <p style={{ fontSize: '0.75rem', color: 'var(--text-muted)', margin: '0 0 var(--space-sm)' }}>
              暂无配置的市场源。添加一个 Git 仓库 URL 或本地路径来获取插件。
            </p>
          )}

          {sources.map(s => (
            <div key={s.name} style={{
              display: 'flex', alignItems: 'center', justifyContent: 'space-between',
              padding: '6px 8px', borderRadius: 'var(--radius-sm)',
              background: 'var(--bg-hover)', marginBottom: '4px', fontSize: '0.78rem',
            }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: '8px', minWidth: 0 }}>
                <span style={{
                  display: 'inline-block', padding: '1px 6px', borderRadius: '3px',
                  fontSize: '0.65rem', background: s.kind === 'git' ? 'var(--accent-light)' : 'var(--bg-secondary)',
                  color: s.kind === 'git' ? 'var(--accent)' : 'var(--text-muted)',
                  flexShrink: 0,
                }}>
                  {s.kind === 'git' ? 'Git' : 'Local'}
                </span>
                <span style={{ fontWeight: 500 }}>{s.name}</span>
                <span style={{ color: 'var(--text-muted)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                  {s.url || s.path || ''}
                </span>
              </div>
              <button
                type="button"
                onClick={() => removeSource(s.name)}
                style={{
                  background: 'none', border: 'none', cursor: 'pointer',
                  color: 'var(--text-muted)', padding: '2px 6px', fontSize: '0.7rem',
                  flexShrink: 0,
                }}
                title="移除此源"
              >
                ✕
              </button>
            </div>
          ))}

          <div style={{ display: 'flex', gap: 'var(--space-xs)', marginTop: 'var(--space-sm)' }}>
            <input
              type="text"
              placeholder="Git URL 或本地路径"
              value={addUrl}
              onChange={e => setAddUrl(e.target.value)}
              onKeyDown={e => { if (e.key === 'Enter') addSource(); }}
              style={{
                flex: 1, padding: '5px 10px', fontSize: '0.78rem',
                border: '1px solid var(--border)', borderRadius: 'var(--radius-sm)',
                background: 'var(--bg)', color: 'var(--text)',
              }}
            />
            <input
              type="text"
              placeholder="名称（可选）"
              value={addName}
              onChange={e => setAddName(e.target.value)}
              onKeyDown={e => { if (e.key === 'Enter') addSource(); }}
              style={{
                width: '120px', padding: '5px 10px', fontSize: '0.78rem',
                border: '1px solid var(--border)', borderRadius: 'var(--radius-sm)',
                background: 'var(--bg)', color: 'var(--text)',
              }}
            />
            <button
              type="button"
              className={styles['settings-save-btn-sm']}
              onClick={addSource}
              disabled={!addUrl.trim()}
              style={{ fontSize: '0.78rem', whiteSpace: 'nowrap' }}
            >
              添加
            </button>
          </div>
        </div>
      )}

      <SettingsSection variant="flush">
        {!marketplace ? (
          <p className={`${styles['settings-muted-note']} ${styles['skills-empty']}`}>
            {t('settings.plugins.marketLoading')}
          </p>
        ) : (
          <>
            {marketplace.warnings && marketplace.warnings.length > 0 && (
              <p className={`${styles['settings-muted-note']} ${styles['skills-empty']}`} style={{ color: 'var(--danger, #c55)' }}>
                {marketplace.warnings[0]}
              </p>
            )}
            {marketplace.plugins.length === 0 ? (
              <p className={`${styles['settings-muted-note']} ${styles['skills-empty']}`}>
                {t('settings.plugins.marketplaceEmpty')}
              </p>
            ) : (
              <div className={styles['plugin-marketplace-grid']}>
                <div className={styles['skills-list-block']}>
                  {marketplace.plugins.map(plugin => (
                    <div
                      key={plugin.id}
                      className={styles['skills-list-item']}
                      onClick={() => selectPlugin(plugin)}
                      style={selectedPlugin?.id === plugin.id ? { background: 'var(--bg-hover)' } : undefined}
                    >
                      <div className={styles['skills-list-info']}>
                        <div style={{ display: 'flex', alignItems: 'center', gap: '6px', flexWrap: 'wrap' }}>
                          <span className={styles['skills-list-name']}>{plugin.name}</span>
                          <span className={styles['skills-list-name-hint']}>v{marketVersion(plugin)}</span>
                          {plugin.installed && (
                            <span className={styles['skills-source-badge']} style={{ marginRight: 0 }}>
                              {t('settings.plugins.marketInstalled')}
                            </span>
                          )}
                          {plugin.updateAvailable && (
                            <span className={styles['skills-source-badge']} style={{ marginRight: 0 }}>
                              {t('settings.plugins.marketUpdateAvailable')}
                            </span>
                          )}
                        </div>
                        {plugin.description && <span className={styles['skills-list-desc']}>{plugin.description}</span>}
                        <span className={styles['skills-list-desc']}>
                          {(plugin.publisher || 'unknown') + ' · ' + (plugin.trust || 'restricted')}
                        </span>
                      </div>
                    </div>
                  ))}
                </div>

                <div className={styles['skills-list-block']}>
                  <div className={styles['skills-list-item']} style={{ alignItems: 'flex-start', cursor: 'default' }}>
                    <div className={styles['skills-list-info']} style={{ gap: 'var(--space-sm)', width: '100%' }}>
                      {selectedPlugin ? (
                        <>
                          <div className={styles['plugin-marketplace-detail-header']}>
                            <div style={{ minWidth: 0 }}>
                              <div className={styles['skills-list-name']}>{selectedPlugin.name}</div>
                              <div className={styles['skills-list-desc']}>
                                {(selectedPlugin.publisher || 'unknown') + ' · v' + marketVersion(selectedPlugin)}
                              </div>
                              {marketVersionStatus(selectedPlugin) && (
                                <div className={styles['skills-list-desc']}>
                                  {marketVersionStatus(selectedPlugin)}
                                </div>
                              )}
                            </div>
                            <button
                              className={styles['settings-save-btn-sm']}
                              disabled={!selectedPlugin.canInstall || installingPluginId === selectedPlugin.id}
                              onClick={(e) => {
                                e.stopPropagation();
                                installPlugin(selectedPlugin);
                              }}
                            >
                              {marketInstallLabel(selectedPlugin)}
                            </button>
                            <button
                              className={styles['settings-save-btn-sm']}
                              disabled={translatingPluginId === selectedPlugin.id || (!readme && !selectedPlugin.description)}
                              onClick={(e) => {
                                e.stopPropagation();
                                translateReadme(selectedPlugin);
                              }}
                              title={t('settings.plugins.translateReadme')}
                            >
                              {translatingPluginId === selectedPlugin.id ? '…' : t('settings.plugins.translate')}
                            </button>
                          </div>
                          <div style={{ display: 'flex', gap: 4, flexWrap: 'wrap' }}>
                            {(selectedPlugin.contributions || []).map(item => (
                              <span key={item} className={styles['skills-source-badge']} style={{ marginRight: 0 }}>
                                {item}
                              </span>
                            ))}
                          </div>
                          <div
                            className={`preview-markdown ${styles['plugin-marketplace-readme']}`}
                            dangerouslySetInnerHTML={{
                              __html: readmeLoading
                                ? `<p>${t('settings.plugins.marketReadmeLoading')}</p>`
                                : renderMarkdown(translatedReadme || readme || selectedPlugin.description || ''),
                            }}
                          />
                        </>
                      ) : (
                        <span className={styles['skills-list-desc']}>{t('settings.plugins.marketSelectPlugin')}</span>
                      )}
                    </div>
                  </div>
                </div>
              </div>
            )}
          </>
        )}
      </SettingsSection>
    </div>
  );
}
