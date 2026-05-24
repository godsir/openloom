import React from 'react';
import { t, autoSaveConfig } from '../../helpers';
import { SettingsSection } from '../../components/SettingsSection';
import styles from '../../Settings.module.css';

export function MemorySection({ memoryEnabled }: {
  memoryEnabled: boolean;
}) {
  const memoryToggle = (
    <button
      className={`hana-toggle${memoryEnabled ? ' on' : ''}`}
      onClick={() => autoSaveConfig({ memory: { enabled: !memoryEnabled } })}
    />
  );

  return (
    <SettingsSection title={t('settings.memory.sectionTitle')} context={memoryToggle}>
      <div style={{ padding: 'var(--space-sm) var(--space-md)' }}>
        <div className={!memoryEnabled ? 'settings-disabled' : ''}>
          <div className={styles['settings-subsection']}>
            <div className={styles['settings-subsection-header']}>
              <h3 className={styles['settings-subsection-title']}>{t('settings.memory.persona')}</h3>
              <span className={styles['settings-subsection-hint']}>{t('settings.memory.personaHint')}</span>
            </div>
            <button
              className={`${styles['memory-action-btn']} ${styles['compiled-view-btn']}`}
              onClick={() => window.dispatchEvent(new Event('hana-view-compiled-memory'))}
            >
              {t('settings.memory.viewPersona')}
            </button>
          </div>

          <div className={styles['settings-subsection']}>
            <h3 className={styles['settings-subsection-title']}>{t('settings.memory.cognitions')}</h3>
            <div className={`${styles['memory-actions-row']} ${styles['memory-actions-spaced']}`}>
              <button
                className={styles['memory-action-btn']}
                onClick={() => window.dispatchEvent(new Event('hana-record-memory'))}
              >
                {t('settings.memory.record')}
              </button>
              <button
                className={styles['memory-action-btn']}
                onClick={() => window.dispatchEvent(new Event('hana-view-memories'))}
              >
                {t('settings.memory.viewCognitions')}
              </button>
              <button
                className={`${styles['memory-action-btn']} ${styles['danger']}`}
                onClick={() => window.dispatchEvent(new Event('hana-show-clear-confirm'))}
              >
                {t('settings.memory.clearCognitions')}
              </button>
            </div>
          </div>
        </div>
      </div>
    </SettingsSection>
  );
}
