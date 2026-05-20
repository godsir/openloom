import React, { useState, useEffect } from 'react';
import ReactMarkdown from 'react-markdown';

interface Message {
    role: 'user' | 'assistant';
    content: string;
}

export default function ChatArea({ sessionId }: { sessionId: string }) {
    const [messages, setMessages] = useState<Message[]>([]);
    const [input, setInput] = useState('');
    const [loading, setLoading] = useState(false);
    const [agentState, setAgentState] = useState('idle');
    const [lastUsage, setLastUsage] = useState<{prompt_tokens?: number; completion_tokens?: number} | null>(null);

    useEffect(() => {
        const unsub1 = window.openloom?.subscribe('agent.state_changed', (data: any) => {
            setAgentState(data.new_state || 'idle');
        });
        const unsub2 = window.openloom?.subscribe('token.usage', (data: any) => {
            setLastUsage(data);
        });
        return () => {
            try { unsub1?.(); } catch {}
            try { unsub2?.(); } catch {}
        };
    }, []);

    async function sendMessage() {
        if (!input.trim() || loading) return;
        const userMsg: Message = { role: 'user', content: input };
        setMessages((prev) => [...prev, userMsg]);
        setInput('');
        setLoading(true);

        try {
            const result = await window.openloom?.send('chat.send', {
                messages: [{ role: 'user', content: input }],
                session_id: sessionId,
                stream: false,
            });
            const content = (result?.response as string) || 'No response';
            setMessages((prev) => [...prev, { role: 'assistant', content }]);
        } catch (err) {
            setMessages((prev) => [
                ...prev,
                { role: 'assistant', content: `Error: ${err}` },
            ]);
        } finally {
            setLoading(false);
        }
    }

    return (
        <div className="flex flex-col h-full">
            <div className="flex-1 overflow-y-auto p-4 space-y-4">
                {messages.map((msg, i) => (
                    <div
                        key={i}
                        className={`flex ${msg.role === 'user' ? 'justify-end' : 'justify-start'}`}
                    >
                        <div
                            className={`max-w-[70%] rounded-lg p-3 ${
                                msg.role === 'user' ? 'bg-blue-600' : 'bg-gray-700'
                            }`}
                        >
                            <ReactMarkdown>{msg.content}</ReactMarkdown>
                        </div>
                    </div>
                ))}
                {loading && <div className="text-gray-400">Thinking...</div>}
            </div>
            <div className="flex items-center gap-2 px-4 py-1 text-xs text-gray-400 border-t border-gray-700">
                <span className={`w-2 h-2 rounded-full ${
                    agentState === 'thinking' ? 'bg-yellow-400 animate-pulse' :
                    agentState === 'acting' ? 'bg-blue-400' : 'bg-gray-500'
                }`} />
                Agent: {agentState}
                {lastUsage && (
                    <span className="ml-auto">
                        ↑{lastUsage.prompt_tokens} ↓{lastUsage.completion_tokens} tokens
                    </span>
                )}
            </div>
            <div className="p-4 border-t border-gray-700">
                <div className="flex gap-2">
                    <input
                        className="flex-1 bg-gray-800 rounded-lg px-4 py-2 outline-none border border-gray-600 focus:border-blue-500"
                        value={input}
                        onChange={(e) => setInput(e.target.value)}
                        onKeyDown={(e) => e.key === 'Enter' && sendMessage()}
                        placeholder="Type a message..."
                        disabled={loading}
                    />
                    <button
                        className="px-6 py-2 bg-blue-600 rounded-lg hover:bg-blue-700 disabled:opacity-50"
                        onClick={sendMessage}
                        disabled={loading}
                    >
                        Send
                    </button>
                </div>
            </div>
        </div>
    );
}
