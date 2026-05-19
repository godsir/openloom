import React, { useState } from 'react';

export default function SettingsPanel() {
    const [config, setConfig] = useState(`[[models]]
name = "router"
path = "qwen3-1.7b-q4_k_m.gguf"
model_type = "Router"
backend = "LlamaCpp"
n_gpu_layers = 32
context_size = 4096`);

    return (
        <div className="p-6">
            <h2 className="text-xl font-bold mb-4">Model Configuration</h2>
            <textarea
                className="w-full h-64 bg-gray-800 p-4 rounded-lg font-mono text-sm border border-gray-600"
                value={config}
                onChange={(e) => setConfig(e.target.value)}
                spellCheck={false}
            />
            <button className="mt-4 px-6 py-2 bg-blue-600 rounded-lg hover:bg-blue-700">
                Save Configuration
            </button>
        </div>
    );
}
