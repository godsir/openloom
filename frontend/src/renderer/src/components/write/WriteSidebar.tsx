import { useStore } from '../../stores'
import { WriteFileTree } from './WriteFileTree';
import { useLocale } from '../../i18n';
import { IconFolder } from '../../utils/icons';
import styles from './WriteSidebar.module.css';

interface WriteSidebarProps {
  onSelectWorkspace: () => void;
  onNewFile: () => void;
}

export function WriteSidebar({ onSelectWorkspace, onNewFile }: WriteSidebarProps) {
  const { t } = useLocale();
  const workspaceRoot = useStore(s => s.appMode) === 'write' ? true : false;
  const writeFileSidebarOpen = useStore(s => s.writeFileSidebarOpen);
  const toggleWriteFileSidebar = useStore(s => s.toggleWriteFileSidebar);

  if (!writeFileSidebarOpen) return null;

  return (
    <aside className={styles.sidebar}>
      <div className={styles.header}>
        <button className={styles.iconBtn} onClick={onSelectWorkspace}
          title={t('write.clickSwitchDir', 'Switch Workspace')}>
          <IconFolder size={14} />
        </button>

        <span className={styles.title}>{t('write.fileList', '文件列表')}</span>

        <button className={styles.iconBtn} onClick={onNewFile}
          title={t('write.newFile', 'New File')}>
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/>
          </svg>
        </button>

        <button className={styles.iconBtn} onClick={toggleWriteFileSidebar}
          title={t('write.collapseSidebar', 'Collapse Sidebar')}>
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <polyline points="15 18 9 12 15 6"/>
          </svg>
        </button>
      </div>

      <div className={styles.treeWrapper}>
        <WriteFileTree onNewFile={onNewFile} />
      </div>
    </aside>
  );
}

export default WriteSidebar;
