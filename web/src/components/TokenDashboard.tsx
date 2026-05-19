import React, { useState, useEffect } from 'react';

interface TokenStats {
    localTokens: number;
    routerHits: number;
    totalRequests: number;
}

export default function TokenDashboard() {
    const [stats, setStats] = useState<TokenStats>({
        localTokens: 0,
        routerHits: 0,
        totalRequests: 0,
    });

    useEffect(() => {
        window.openloom?.subscribe('token.usage', (data: Record<string, unknown>) => {
            setStats((prev) => ({
                ...prev,
                localTokens: prev.localTokens + ((data.prompt_tokens as number) || 0),
                totalRequests: prev.totalRequests + 1,
            }));
        });
    }, []);

    const savingsRate = stats.totalRequests > 0
        ? ((stats.routerHits / stats.totalRequests) * 100).toFixed(1)
        : '0.0';

    return (
        <div className="p-6">
            <h2 className="text-xl font-bold mb-4">Token Monitor</h2>
            <div className="grid grid-cols-2 gap-4">
                <div className="bg-gray-800 p-4 rounded-lg">
                    <div className="text-sm text-gray-400">Local Token Usage</div>
                    <div className="text-2xl font-bold">{stats.localTokens.toLocaleString()}</div>
                </div>
                <div className="bg-gray-800 p-4 rounded-lg">
                    <div className="text-sm text-gray-400">Router Hit Rate</div>
                    <div className="text-2xl font-bold">{savingsRate}%</div>
                </div>
                <div className="bg-gray-800 p-4 rounded-lg">
                    <div className="text-sm text-gray-400">Total Requests</div>
                    <div className="text-2xl font-bold">{stats.totalRequests}</div>
                </div>
                <div className="bg-gray-800 p-4 rounded-lg">
                    <div className="text-sm text-gray-400">Router Handled</div>
                    <div className="text-2xl font-bold">{stats.routerHits}</div>
                </div>
            </div>
        </div>
    );
}
