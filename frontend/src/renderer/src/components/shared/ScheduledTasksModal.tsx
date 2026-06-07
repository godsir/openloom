import Overlay from './Overlay'
import CronTab from '../settings/CronTab'
import styles from './ScheduledTasksModal.module.css'

export default function ScheduledTasksModal({ open, onClose }: { open: boolean; onClose: () => void }) {
  return (
    <Overlay open={open} onClose={onClose} size="lg">
      <div className={styles.container}>
        <CronTab />
      </div>
    </Overlay>
  )
}
