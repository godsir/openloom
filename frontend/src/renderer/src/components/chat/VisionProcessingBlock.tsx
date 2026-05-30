import { useState } from 'react'
import { IconEye, IconChevronRight, IconChevronDown, IconCheck, IconLoader, IconXCircle } from '../../utils/icons'
import styles from './VisionProcessingBlock.module.css'

interface BatchInfo {
  batchIndex: number
  totalBatches: number
  status: 'running' | 'done' | 'error'
  result?: string
}

interface Props {
  block: {
    type: string
    content?: string
    status?: 'running' | 'waiting' | 'done'
    batches?: BatchInfo[]
  }
}

function BatchStatusIcon({ status }: { status: string }) {
  if (status === 'done') return <IconCheck size={10} className={styles.batchIconDone} />
  if (status === 'running') return <IconLoader size={10} className={styles.batchIconRunning} />
  return <IconXCircle size={10} className={styles.batchIconError} />
}

export default function VisionProcessingBlock({ block }: Props) {
  const [expanded, setExpanded] = useState(false)
  const status = block.status ?? 'running'
  const batches = block.batches ?? []
  const hasBatches = batches.length > 0
  const doneCount = batches.filter(b => b.status === 'done').length
  const totalCount = hasBatches ? batches[0].totalBatches : 0
  const allDone = status === 'done'

  const label = hasBatches && totalCount > 1
    ? `图片分析 ${doneCount}/${totalCount}`
    : block.content || (status === 'waiting' ? '辅助视觉已完成，主模型生成中' : '辅助视觉正在处理图片')

  return (
    <div className={`${styles.block} ${allDone ? styles.blockDone : ''}`}>
      <button
        className={styles.header}
        onClick={() => hasBatches && setExpanded(!expanded)}
        disabled={!hasBatches}
      >
        <IconEye size={14} className={`${styles.icon} ${!allDone ? styles.iconPulse : ''}`} />
        <span className={styles.label}>{label}</span>
        {!allDone && (
          <span className={styles.dots}>
            <span className={styles.dot} />
            <span className={styles.dot} />
            <span className={styles.dot} />
          </span>
        )}
        {hasBatches && (
          <span className={styles.chevron}>
            {expanded ? <IconChevronDown size={10} /> : <IconChevronRight size={10} />}
          </span>
        )}
      </button>

      {expanded && hasBatches && (
        <div className={styles.body}>
          {batches
            .slice()
            .sort((a, b) => a.batchIndex - b.batchIndex)
            .map((batch) => (
              <div key={batch.batchIndex} className={styles.batchRow}>
                <BatchStatusIcon status={batch.status} />
                <span className={styles.batchLabel}>
                  第 {batch.batchIndex + 1} 批
                  {batch.totalBatches > 1 && ` / ${batch.totalBatches}`}
                </span>
                {batch.result && (
                  <div className={styles.batchResult}>{batch.result}</div>
                )}
              </div>
            ))}
        </div>
      )}
    </div>
  )
}
