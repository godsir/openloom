import { useStore } from '../../stores'
import { useWriteStore } from '../../stores/write'
import { WriteFileTree } from './WriteFileTree';
import { useLocale } from '../../i18n';
import { IconFolder, IconChevronLeft } from '../../utils/icons';
import styles from './WriteSidebar.module.css';

interface WriteSidebarProps {
  onSelectWorkspace: () => void;
  onNewFile: () => void;
}

export function WriteSidebar({ onSelectWorkspace, onNewFile }: WriteSidebarProps) {
  const { t } = useLocale();
  const workspaceRoot = useWriteStore(s => s.workspaceRoot);
  const writeFileSidebarOpen = useStore(s => s.writeFileSidebarOpen);
  const toggleWriteFileSidebar = useStore(s => s.toggleWriteFileSidebar);

  if (!writeFileSidebarOpen) return null;

  const displayPath = workspaceRoot
    ? (workspaceRoot.length > 28 ? '...' + workspaceRoot.slice(-25) : workspaceRoot)
    : '';

  return (
    <aside className={styles.sidebar}>
      <div className={styles.header}>
        <span className={styles.title}>{t('write.fileList', '文件列表')}</span>
        <button className={styles.iconBtn} onClick={onNewFile}
          title={t('write.newFile', 'New File')}>
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/>
          </svg>
        </button>
        <button className={styles.iconBtn} onClick={toggleWriteFileSidebar}
          title={t('write.collapseSidebar', 'Collapse Sidebar')} style={{ marginRight: 4 }}>
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <polyline points="15 18 9 12 15 6"/>
          </svg>
        </button>
      </div>

      <button className={styles.workspaceBar} onClick={onSelectWorkspace}
        title={t('write.clickSwitchDir', '点击切换工作目录')}>
        <IconFolder size={14} />
        <span className={styles.workspacePath}>{displayPath}</span>
        <span className={styles.workspaceHint}>{t('write.clickSwitchDir', '切换')}</span>
      </button>

      <div className={styles.treeWrapper}>
        <WriteFileTree onNewFile={onNewFile} />
      </div>
    </aside>
  );
}

export default WriteSidebar;
