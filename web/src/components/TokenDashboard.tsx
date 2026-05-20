import React, { useState, useEffect } from 'react';

interface TokenCall {
    model: string;
    prompt_tokens: number;
    completion_tokens: number;
    cached_tokens: number;
    latency_ms: number;
    time: Date;
}

export default function TokenDashboard() {
    const [totalPrompt, setTotalPrompt] = useState(0);
    const [totalCompletion, setTotalCompletion] = useState(0);
    const [totalCached, setTotalCached] = useState(0);
    const [recentCalls, setRecentCalls] = useState<TokenCall[]>([]);

    useEffect(() => {
        const unsub = window.openloom?.subscribe('token.usage', (data: any) => {
            const p = data.prompt_tokens || 0;
            const c = data.completion_tokens || 0;
            const cached = data.cached_tokens || 0;
            setTotalPrompt(prev => prev + p);
            setTotalCompletion(prev => prev + c);
            setTotalCached(prev => prev + cached);
            setRecentCalls(prev => [{
                model: data.model || 'unknown',
                prompt_tokens: p,
                completion_tokens: c,
                cached_tokens: cached,
                latency_ms: data.latency_ms || 0,
                time: new Date(),
            }, ...prev].slice(0, 50));
        });
        return () => { try { unsub?.(); } catch {} };
    }, []);

    const totalTokens = totalPrompt + totalCompletion;
    const savingsRate = totalTokens > 0 ? ((totalCached / totalTokens) * 100).toFixed(1) : '0.0';

    return (
        <div className="p-6 overflow-y-auto">
            <h2 className="text-xl font-bold mb-4">Token Dashboard</h2>

            <div className="grid grid-cols-2 gap-4 mb-6">
                <StatCard label="Total Prompt" value={totalPrompt.toLocaleString()} />
                <StatCard label="Total Completion" value={totalCompletion.toLocaleString()} />
                <StatCard label="Cached Tokens" value={totalCached.toLocaleString()} />
                <StatCard label="Cache Hit Rate" value={`${savingsRate}%`} />
            </div>

            <h3 className="text-lg font-semibold mb-2">Recent Calls</h3>
            <div className="space-y-1 max-h-96 overflow-y-auto">
                {recentCalls.length === 0 && <p className="text-gray-500 text-sm">No data yet. Send a message to start.</p>}
                {recentCalls.map((call, i) => (
                    <div key={i} className="flex justify-between text-sm p-2 bg-gray-800 rounded">
                        <span>{call.model}</span>
                        <span className="text-gray-400">↑{call.prompt_tokens} ↓{call.completion_tokens}</span>
                        <span className="text-gray-500">{call.latency_ms}ms</span>
                        <span className="text-gray-600 text-xs">{call.time.toLocaleTimeString()}</span>
                    </div>
                ))}
            </div>
        </div>
    );
}

function StatCard({ label, value }: { label: string; value: string }) {
    return (
        <div className="p-4 bg-gray-800 rounded-lg">
            <div className="text-gray-400 text-xs mb-1">{label}</div>
            <div className="text-2xl font-bold">{value}</div>
        </div>
    );
}
