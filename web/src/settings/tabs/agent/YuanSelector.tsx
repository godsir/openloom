import React from 'react';
import { t } from '../../helpers';

const YUAN_COLORS: Record<string, string> = {
  hanako: '#537D96',
  butter: '#5BA88C',
  ming:   '#8BA4B4',
  kong:   '#555555',
};

export function YuanSelector({ currentYuan, onChange }: { currentYuan: string; onChange: (key: string) => void }) {
  const types = t('yuan.types') || {};
  const entries = Object.entries(types) as [string, { label?: string; desc?: string; avatar?: string }][];
  const hIdx = entries.findIndex(([k]) => k === 'hanako');
  if (hIdx >= 0 && entries.length >= 3) {
    const [h] = entries.splice(hIdx, 1);
    entries.splice(1, 0, h);
  }

  return (
    <div className="yuan-selector">
      <div className="yuan-chips">
        {entries.map(([key, meta]) => (
          <button
            key={key}
            className={`yuan-chip${key === currentYuan ? ' selected' : ''}`}
            type="button"
            onClick={() => { if (key !== currentYuan) onChange(key); }}
          >
            <div className="yuan-chip-avatar" style={{
              background: YUAN_COLORS[key] || YUAN_COLORS.hanako,
              display: 'flex', alignItems: 'center', justifyContent: 'center',
              color: '#fff', fontWeight: 700, fontSize: 11,
              width: 46, height: 46, borderRadius: '50%', flexShrink: 0,
            }}>
              Loom
            </div>
            <div className="yuan-chip-info">
              <span className="yuan-chip-name">{meta.label || key}</span>
              <span className="yuan-chip-desc">{meta.desc || ''}</span>
            </div>
          </button>
        ))}
      </div>
    </div>
  );
}
