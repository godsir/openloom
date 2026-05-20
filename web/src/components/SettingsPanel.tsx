import React, { useState, useEffect } from 'react';

interface ConfigData {
    server?: { host?: string };
    logging?: { level?: string };
    agent?: { max_iterations?: number; timeout_secs?: number };
    persona?: { top_n?: number; recency_decay_days?: number };
    rate_limit?: { min_interval_ms?: number };
}

export default function SettingsPanel() {
    const [config, setConfig] = useState<ConfigData | null>(null);
    const [saved, setSaved] = useState(false);

    useEffect(() => {
        window.openloom?.send('config.get').then((data: any) => {
            setConfig(data.config || {});
        }).catch(() => setConfig({}));
    }, []);

    const updateField = async (key: string, value: string | number) => {
        await window.openloom?.send('config.set', { key, value: String(value) });
        setSaved(true);
        setTimeout(() => setSaved(false), 2000);
    };

    if (!config) return <div className="p-6 text-gray-400">Loading config...</div>;

    return (
        <div className="p-6 overflow-y-auto">
            <h2 className="text-xl font-bold mb-4">Settings</h2>
            {saved && <div className="mb-4 p-2 bg-green-800 rounded text-sm">Saved</div>}

            <Section title="Server">
                <Field label="Host" value={config.server?.host || '127.0.0.1'}
                    onChange={v => updateField('server.host', v)} />
            </Section>

            <Section title="Logging">
                <Field label="Level" value={config.logging?.level || 'INFO'}
                    onChange={v => updateField('logging.level', v)} />
            </Section>

            <Section title="Agent">
                <Field label="Max Iterations" value={config.agent?.max_iterations ?? 3}
                    onChange={v => updateField('agent.max_iterations', Number(v))} type="number" />
                <Field label="Timeout (seconds)" value={config.agent?.timeout_secs ?? 120}
                    onChange={v => updateField('agent.timeout_secs', Number(v))} type="number" />
            </Section>

            <Section title="Persona">
                <Field label="Top N Traits" value={config.persona?.top_n ?? 5}
                    onChange={v => updateField('persona.top_n', Number(v))} type="number" />
                <Field label="Recency Decay (days)" value={config.persona?.recency_decay_days ?? 30}
                    onChange={v => updateField('persona.recency_decay_days', Number(v))} type="number" />
            </Section>

            <Section title="Rate Limit">
                <Field label="Min Interval (ms)" value={config.rate_limit?.min_interval_ms ?? 100}
                    onChange={v => updateField('rate_limit.min_interval_ms', Number(v))} type="number" />
            </Section>
        </div>
    );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
    return (
        <div className="mb-6">
            <h3 className="text-sm font-semibold text-gray-400 mb-2 uppercase">{title}</h3>
            <div className="space-y-3">{children}</div>
        </div>
    );
}

function Field({ label, value, onChange, type = 'text' }: {
    label: string; value: string | number; onChange: (v: string) => void; type?: string;
}) {
    return (
        <div className="flex items-center justify-between">
            <label className="text-sm">{label}</label>
            <input
                type={type}
                value={value}
                onChange={e => onChange(e.target.value)}
                className="bg-gray-800 border border-gray-600 rounded px-3 py-1 w-48 text-sm outline-none focus:border-blue-500"
            />
        </div>
    );
}
