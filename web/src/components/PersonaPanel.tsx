import React, { useState, useEffect } from 'react';

interface Trait {
    trait: string;
    value: string;
    confidence: number;
    evidence_count: number;
}

export default function PersonaPanel() {
    const [summary, setSummary] = useState('');
    const [traits, setTraits] = useState<Trait[]>([]);
    const [loading, setLoading] = useState(true);

    const fetchData = async () => {
        setLoading(true);
        try {
            const p: any = await window.openloom?.send('memory.persona');
            setSummary(p.summary || '');
            const c: any = await window.openloom?.send('memory.cognitions', { subject: 'USER', limit: 20 });
            setTraits(c.cognitions || []);
        } catch (e) {
            console.error('Failed to load persona:', e);
        } finally {
            setLoading(false);
        }
    };

    useEffect(() => { fetchData(); }, []);

    return (
        <div className="p-6 overflow-y-auto">
            <div className="flex items-center justify-between mb-4">
                <h2 className="text-xl font-bold">Persona Profile</h2>
                <button onClick={fetchData} className="px-4 py-1 bg-blue-600 rounded text-sm hover:bg-blue-700">
                    Refresh
                </button>
            </div>

            {loading ? (
                <p className="text-gray-400">Loading...</p>
            ) : (
                <>
                    <div className="mb-6 p-4 bg-gray-800 rounded-lg">
                        <p className="text-lg">{summary || 'No persona data yet. Interact more to build a cognition profile.'}</p>
                    </div>

                    <h3 className="text-lg font-semibold mb-2">Traits</h3>
                    {traits.length === 0 ? (
                        <p className="text-gray-500">No cognitive traits discovered yet.</p>
                    ) : (
                        <div className="space-y-2">
                            {traits.map((t, i) => (
                                <div key={i} className="flex items-center justify-between p-3 bg-gray-800 rounded">
                                    <div>
                                        <span className="font-medium">{t.trait}</span>
                                        <span className="text-gray-400 ml-2">{t.value}</span>
                                    </div>
                                    <div className="flex items-center gap-4 text-sm text-gray-400">
                                        <span>{(t.confidence * 100).toFixed(0)}% confidence</span>
                                        <span>{t.evidence_count} events</span>
                                    </div>
                                </div>
                            ))}
                        </div>
                    )}
                </>
            )}
        </div>
    );
}
