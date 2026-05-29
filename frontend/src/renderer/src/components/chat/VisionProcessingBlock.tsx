import { IconEye } from '../../utils/icons'
import styles from './VisionProcessingBlock.module.css'

export default function VisionProcessingBlock({
  block,
}: {
  block: { type: string; content?: string; status?: 'running' | 'waiting' }
}) {
  const status = block.status ?? 'running'
  const label =
    block.content || (status === 'waiting' ? '辅助视觉已完成，主模型生成中' : '辅助视觉正在处理图片')
  return (
    <div className={styles.block} data-status={status}>
      <IconEye size={14} className={styles.icon} />
      <span className={styles.label}>{label}</span>
      <span className={styles.dots}>
        <span className={styles.dot} />
        <span className={styles.dot} />
        <span className={styles.dot} />
      </span>
    </div>
  )
}
