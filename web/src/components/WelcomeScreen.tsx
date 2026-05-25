import { useStore } from '../stores';
import styles from './Welcome.module.css';
import iconPng from '../assets/icon.png';

export function WelcomeScreen() {
  const agentName = useStore(s => s.agentName);
  const agentYuan = useStore(s => s.agentYuan);
  const userName = useStore(s => s.userName);

  return (
    <div className={styles.welcome}>
      <img className={styles.welcomeAvatar} src={iconPng} alt="openLoom" draggable={false} />
      <h1 style={{
        fontSize: '1.3rem',
        fontWeight: 600,
        color: 'var(--text)',
        margin: 0,
        letterSpacing: '0.04em',
        fontFamily: 'Georgia, serif',
      }}>openLoom</h1>
      {agentName && (
        <p className={styles.welcomeText}>
          {agentYuan === 'loom' ? `Hi, ${userName || 'there'} — ${agentName} 就绪` : `${agentName} 已就绪`}
        </p>
      )}
    </div>
  );
}
