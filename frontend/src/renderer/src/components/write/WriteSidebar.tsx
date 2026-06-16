import { useWriteStore } from '../../stores/write';
import { WriteFileTree } from './WriteFileTree';
import { useTranslation } from 'react-i18next';
import { FolderOpen, ChevronLeft } from 'lucide-react';
import styles from './WriteSidebar.module.css';

interface WriteSidebarProps {
  onSelectWorkspace: () => void;
}

export function WriteSidebar({ onSelectWorkspace }: WriteSidebarProps) {
  const { t } = useTranslation();
  const workspaceRoot = useWriteStore((s) => s.workspaceRoot);
  const fileSidebarOpen = useWriteStore((s) => s.fileSidebarOpen);
  const toggleFileSidebar = useWriteStore((s) => s.toggleFileSidebar);

  if (!fileSidebarOpen || !workspaceRoot) {
    return null;
  }

  return (
    <aside className={styles.sidebar}>
      <div className={styles.header}>
        <button
          className={styles.iconBtn}
          onClick={onSelectWorkspace}
          title={t('write.switchWorkspace', 'Switch Workspace')}
          aria-label={t('write.switchWorkspace', 'Switch Workspace')}
        >
          <FolderOpen size={16} />
        </button>

        <span className={styles.title}>
          {t('write.fileExplorer', 'Files')}
        </span>

        <button
          className={styles.iconBtn}
          onClick={toggleFileSidebar}
          title={t('write.collapseSidebar', 'Collapse Sidebar')}
          aria-label={t('write.collapseSidebar', 'Collapse Sidebar')}
        >
          <ChevronLeft size={16} />
        </button>
      </div>

      <div className={styles.treeWrapper}>
        <WriteFileTree />
      </div>
    </aside>
  );
}

export default WriteSidebar;
