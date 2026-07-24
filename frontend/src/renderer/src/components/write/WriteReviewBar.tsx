// AI inline 编辑的审查栏：展示待确认的修改（红删绿增），提供接受/拒绝。
// 挂载在 WriteInlineAgent 工具栏下方，reviewActive 时可见。

import React, { useMemo, useState } from 'react';
import { diffChars } from 'diff';
import { useWriteStore } from '../../stores/write';
import { useLocale } from '../../i18n';
import { acceptInlineEdit, rejectInlineEdit } from '../../write/inline-edit-service';
import styles from './WriteReviewBar.module.css';

// diff 视图渲染上限：超长编辑退化为纯新文本预览，避免 char diff 卡顿
const DIFF_RENDER_LIMIT = 20_000;

export const WriteReviewBar: React.FC = () => {
  const { t } = useLocale();
  const chunks = useWriteStore((s) => s.pendingAgentReview);
  const reviewActive = useWriteStore((s) => s.reviewActive);
  const [expanded, setExpanded] = useState(true);

  const chunk = chunks && chunks.length > 0 ? chunks[0] : null;

  const diffParts = useMemo(() => {
    if (!chunk) return null;
    if (chunk.originalText.length + chunk.modifiedText.length > DIFF_RENDER_LIMIT) return null;
    return diffChars(chunk.originalText, chunk.modifiedText);
  }, [chunk]);

  if (!reviewActive || !chunk) return null;

  const handleAccept = () => {
    const result = acceptInlineEdit();
    if (!result.ok && result.message) {
      useWriteStore.getState().showToast('error', result.message);
    }
  };

  const handleReject = () => {
    rejectInlineEdit();
  };

  return (
    <div className={styles.reviewWrap}>
      <div className={styles.bar}>
        <span className={styles.title}>{t('write.reviewTitle')}</span>
        <span className={styles.stats}>
          <span className={styles.statDel}>-{chunk.originalText.length}</span>
          <span className={styles.statAdd}>+{chunk.modifiedText.length}</span>
        </span>
        <span className={styles.spacer} />
        <button className={styles.btn} onClick={() => setExpanded((e) => !e)}>
          {expanded ? t('write.reviewHideDiff') : t('write.reviewShowDiff')}
        </button>
        <button className={styles.acceptBtn} onClick={handleAccept}>
          {t('write.reviewAccept')}
        </button>
        <button className={styles.rejectBtn} onClick={handleReject}>
          {t('write.reviewReject')}
        </button>
      </div>
      {expanded && (
        <div className={styles.diffPanel}>
          {diffParts ? (
            <pre className={styles.diffText}>
              {diffParts.map((part, i) => (
                <span
                  key={i}
                  className={
                    part.added ? styles.diffAdd : part.removed ? styles.diffDel : undefined
                  }
                >
                  {part.value}
                </span>
              ))}
            </pre>
          ) : (
            <pre className={styles.diffText}>{chunk.modifiedText}</pre>
          )}
        </div>
      )}
    </div>
  );
};

export default WriteReviewBar;
