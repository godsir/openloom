import React, { useState, useEffect } from 'react';

interface Cognition {
    id: number;
    trait: string;
    value: string;
    confidence: number;
    evidence_count: number;
    version: number;
}

interface Snapshot {
    id: number;
    cognition_id: number;
    version: number;
    trait_name: string;
    value: string;
    confidence: number;
    evidence_count: number;
    snapshot_at: number;
}

export default function CognitionAuditPanel() {
    const [cognitions, setCognitions] = useState<Cognition[]>([]);
    const [loading, setLoading] = useState(true);
    const [selectedId, setSelectedId] = useState<number | null>(null);
    const [snapshots, setSnapshots] = useState<Snapshot[]>([]);
    const [snapLoading, setSnapLoading] = useState(false);
    const [error, setError] = useState('');

    const fetchCognitions = async () => {
        setLoading(true);
        setError('');
        try {
            const data: any = await window.openloom?.send('memory.cognitions', { subject: 'USER', limit: 50 });
            setCognitions(data.cognitions || []);
            // Refresh snapshots if a cognition is selected
            if (selectedId !== null) {
                fetchSnapshots(selectedId);
            }
        } catch (e: any) {
            setError('Failed to load cognitions: ' + (e.message || e));
        } finally {
            setLoading(false);
        }
    };

    const fetchSnapshots = async (cognitionId: number) => {
        setSnapLoading(true);
        setError('');
        try {
            const data: any = await window.openloom?.send('memory.cognition_snapshots', { cognition_id: cognitionId });
            setSnapshots(data.snapshots || []);
        } catch (e: any) {
            setError('Failed to load snapshots: ' + (e.message || e));
        } finally {
            setSnapLoading(false);
        }
    };

    const handleSelect = (id: number) => {
        setSelectedId(id);
        fetchSnapshots(id);
    };

    const handleRollback = async (cognitionId: number, version: number) => {
        setError('');
        try {
            await window.openloom?.send('memory.cognition_rollback', { cognition_id: cognitionId, version });
            await fetchCognitions();
        } catch (e: any) {
            setError('Rollback failed: ' + (e.message || e));
        }
    };

    useEffect(() => { fetchCognitions(); }, []);

    const selectedCog = cognitions.find((c) => c.id === selectedId);

    return (
        <div className="p-6 overflow-y-auto h-full">
            <div className="flex items-center justify-between mb-4">
                <h2 className="text-xl font-bold">Cognition Audit</h2>
                <button onClick={fetchCognitions} className="px-4 py-1 bg-blue-600 rounded text-sm hover:bg-blue-700">
                    Refresh
                </button>
            </div>

            {error && (
                <div className="mb-4 p-3 bg-red-900/40 border border-red-700 rounded text-sm text-red-300">
                    {error}
                </div>
            )}

            {loading ? (
                <p className="text-gray-400">Loading...</p>
            ) : cognitions.length === 0 ? (
                <p className="text-gray-500">No cognitions found. Interact more to build a cognition profile.</p>
            ) : (
                <div className="flex gap-6">
                    {/* Cognition list */}
                    <div className="flex-1 space-y-2">
                        <h3 className="text-lg font-semibold mb-3">Cognitions</h3>
                        {cognitions.map((c) => (
                            <div
                                key={c.id}
                                className={`p-3 rounded cursor-pointer transition-colors ${
                                    selectedId === c.id
                                        ? 'bg-blue-900/40 border border-blue-600'
                                        : 'bg-gray-800 border border-gray-700 hover:border-gray-500'
                                }`}
                                onClick={() => handleSelect(c.id)}
                            >
                                <div className="flex items-center justify-between">
                                    <div>
                                        <span className="font-medium">{c.trait}</span>
                                        <span className="text-gray-400 ml-2 text-xs">v{c.version}</span>
                                    </div>
                                    <span className="text-xs text-gray-500">ID: {c.id}</span>
                                </div>
                                <div className="text-sm text-gray-300 mt-1">{c.value}</div>
                                <div className="flex items-center gap-4 mt-1 text-xs text-gray-400">
                                    <span>{(c.confidence * 100).toFixed(0)}% confidence</span>
                                    <span>{c.evidence_count} events</span>
                                </div>
                            </div>
                        ))}
                    </div>

                    {/* Snapshot / version history panel */}
                    <div className="flex-1">
                        <h3 className="text-lg font-semibold mb-3">Version History</h3>
                        {selectedId === null ? (
                            <p className="text-gray-500">Select a cognition to view its version history.</p>
                        ) : snapLoading ? (
                            <p className="text-gray-400">Loading snapshots...</p>
                        ) : (
                            <div>
                                {/* Current version */}
                                {selectedCog && (
                                    <div className="mb-4 p-3 bg-gray-800 rounded border border-green-700">
                                        <div className="flex items-center justify-between">
                                            <span className="text-sm font-semibold text-green-400">
                                                Current (v{selectedCog.version})
                                            </span>
                                        </div>
                                        <div className="text-sm mt-1">
                                            <span className="font-medium">{selectedCog.trait}:</span>{' '}
                                            {selectedCog.value}
                                        </div>
                                        <div className="text-xs text-gray-400 mt-1">
                                            confidence: {(selectedCog.confidence * 100).toFixed(0)}% | evidence: {selectedCog.evidence_count}
                                        </div>
                                    </div>
                                )}

                                {/* Snapshots */}
                                {snapshots.length === 0 ? (
                                    <p className="text-gray-500 text-sm">No previous versions available.</p>
                                ) : (
                                    <div className="space-y-2">
                                        {snapshots.map((s) => (
                                            <div
                                                key={s.id}
                                                className="p-3 bg-gray-800 rounded border border-gray-700 flex items-center justify-between"
                                            >
                                                <div>
                                                    <div className="flex items-center gap-2">
                                                        <span className="text-sm font-semibold text-yellow-400">
                                                            v{s.version}
                                                        </span>
                                                        <span className="text-xs text-gray-500">
                                                            {new Date(s.snapshot_at * 1000).toLocaleString()}
                                                        </span>
                                                    </div>
                                                    <div className="text-sm mt-1">
                                                        <span className="font-medium">{s.trait_name}:</span>{' '}
                                                        {s.value}
                                                    </div>
                                                    <div className="text-xs text-gray-400 mt-1">
                                                        confidence: {(s.confidence * 100).toFixed(0)}% | evidence: {s.evidence_count}
                                                    </div>
                                                </div>
                                                <button
                                                    className="px-3 py-1 bg-yellow-700 hover:bg-yellow-600 rounded text-xs font-medium transition-colors"
                                                    onClick={() => handleRollback(s.cognition_id, s.version)}
                                                >
                                                    Rollback
                                                </button>
                                            </div>
                                        ))}
                                    </div>
                                )}
                            </div>
                        )}
                    </div>
                </div>
            )}
        </div>
    );
}
