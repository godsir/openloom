/**
 * AppTitlebar.tsx — 自定义标题栏
 *
 * - Windows/Linux: 显示拖动区域 + 自绘窗口控制按钮
 * - 侧边栏/Jian 切换按钮、浮动预览卡触发
 * - macOS 使用 hiddenInset，不需要自绘控件（空占位）
 */
import { useEffect, useState, useCallback } from 'react';

interface AppTitlebarProps {
  sidebarOpen?: boolean;
  jianOpen?: boolean;
  onToggleSidebar?: () => void;
  onToggleJian?: () => void;
  onLeftMouseEnter?: (e: React.MouseEvent<HTMLButtonElement>) => void;
  onRightMouseEnter?: (e: React.MouseEvent<HTMLButtonElement>) => void;
  onToggleMouseLeave?: () => void;
}

function WindowControls() {
  const t = (window as any).t ?? ((k: string) => k);
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    const p = (window as any).hana;
    if (!p) return;
    // Listen for maximize changes from main process
    if (p.onMaximizeChange) {
      const unsub = p.onMaximizeChange((val: boolean) => setMaximized(val));
      return () => unsub?.();
    }
  }, []);

  const minimize = useCallback(() => (window as any).hana?.windowMinimize?.(), []);
  const maximize = useCallback(() => (window as any).hana?.windowMaximize?.(), []);
  const close = useCallback(() => (window as any).hana?.windowClose?.(), []);

  return (
    <div className="window-controls" style={{ display: 'flex', alignItems: 'center', gap: 0 }}>
      <button className="wc-btn wc-minimize" title={t('window.minimize')} onClick={minimize}>
        <svg width="12" height="12" viewBox="0 0 12 12">
          <line x1="2" y1="6" x2="10" y2="6" stroke="currentColor" strokeWidth="1.2"/>
        </svg>
      </button>
      <button className="wc-btn wc-maximize" title={t('window.maximize')} onClick={maximize}>
        <svg width="12" height="12" viewBox="0 0 12 12">
          {maximized
            ? <><rect x="3" y="1" width="7" height="7" fill="none" stroke="currentColor" strokeWidth="1.2"/><rect x="1" y="3" width="7" height="7" fill="none" stroke="currentColor" strokeWidth="1.2"/></>
            : <rect x="2" y="2" width="8" height="8" fill="none" stroke="currentColor" strokeWidth="1.2"/>
          }
        </svg>
      </button>
      <button className="wc-btn wc-close" title={t('window.close')} onClick={close}>
        <svg width="12" height="12" viewBox="0 0 12 12">
          <line x1="2" y1="2" x2="10" y2="10" stroke="currentColor" strokeWidth="1.2"/>
          <line x1="10" y1="2" x2="2" y2="10" stroke="currentColor" strokeWidth="1.2"/>
        </svg>
      </button>
    </div>
  );
}

export function AppTitlebar({
  sidebarOpen,
  jianOpen,
  onToggleSidebar,
  onToggleJian,
  onLeftMouseEnter,
  onRightMouseEnter,
  onToggleMouseLeave,
}: AppTitlebarProps) {
  const [isMac, setIsMac] = useState(false);
  const [isDesktop, setIsDesktop] = useState(false);

  useEffect(() => {
    const hana = (window as any).hana;
    if (!hana) return;
    setIsDesktop(true);
    hana.getPlatform?.().then((p: string) => {
      setIsMac(p === 'darwin');
    }).catch(() => {});
  }, []);

  // Web / non-desktop: no titlebar
  if (!isDesktop) return null;

  return (
    <div className="titlebar">
      {/* Left: macOS traffic light placeholder (mac) OR sidebar toggle (win) */}
      <div className="titlebar-left titlebar-no-drag">
        {isMac ? (
          // macOS: leave space for traffic lights
          <div style={{ width: '72px' }} />
        ) : (
          <button
            className="tb-toggle"
            onMouseEnter={onLeftMouseEnter}
            onMouseLeave={onToggleMouseLeave}
            onClick={onToggleSidebar}
            title="Toggle sidebar"
          >
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <line x1="3" y1="6" x2="21" y2="6"/><line x1="3" y1="12" x2="21" y2="12"/><line x1="3" y1="18" x2="21" y2="18"/>
            </svg>
          </button>
        )}
      </div>

      {/* Center: drag region / app name */}
      <div className="titlebar-drag-center">
        openLoom
      </div>

      {/* Right: Windows controls */}
      <div className="titlebar-right titlebar-no-drag">
        {!isMac && <WindowControls />}
      </div>
    </div>
  );
}
