import { useState } from 'react'
import { Play } from 'lucide-react'
import { useLocale } from '../../i18n'
import { sendContinuation } from '../../services/sendMessage'
import styles from './ContinueButton.module.css'

interface ContinueButtonProps {
  sessionId: string
  disabled?: boolean
}

export default function ContinueButton({ sessionId, disabled = false }: ContinueButtonProps) {
  const [loading, setLoading] = useState(false)
  const { t } = useLocale()

  const handleContinue = async () => {
    if (loading || disabled) return
    setLoading(true)
    try {
      await sendContinuation(sessionId)
    } finally {
      setLoading(false)
    }
  }

  return (
    <button
      className={styles.continueBtn}
      onClick={handleContinue}
      disabled={loading || disabled}
      title={t('chat.continue')}
    >
      <Play size={14} />
      <span>{loading ? t('chat.continuing') : t('chat.continue')}</span>
    </button>
  )
}
