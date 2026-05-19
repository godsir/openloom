import React from 'react';

interface Session {
    id: string;
    name: string;
    messageCount: number;
}

type View = 'chat' | 'settings' | 'dashboard';

interface Props {
    sessions: Session[];
    activeSession: string;
    onSelectSession: (id: string) => void;
    onNewSession: () => void;
    onNavigate: (view: View) => void;
    activeView: View;
}

export default function Sidebar({
    sessions,
    activeSession,
    onSelectSession,
    onNewSession,
    onNavigate,
    activeView,
}: Props) {
    const navItems: { view: View; label: string; icon: string }[] = [
        { view: 'chat', label: 'Chat', icon: '💬' },
        { view: 'dashboard', label: 'Dashboard', icon: '📊' },
        { view: 'settings', label: 'Settings', icon: '⚙️' },
    ];

    return (
        <div className="w-64 bg-gray-800 flex flex-col border-r border-gray-700">
            <div className="p-4">
                <button
                    className="w-full py-2 bg-blue-600 rounded-lg hover:bg-blue-700 text-sm font-medium"
                    onClick={onNewSession}
                >
                    + New Session
                </button>
            </div>
            <div className="flex-1 overflow-y-auto">
                {sessions.map((s) => (
                    <div
                        key={s.id}
                        className={`px-4 py-2 cursor-pointer text-sm ${
                            s.id === activeSession ? 'bg-gray-700' : 'hover:bg-gray-700'
                        }`}
                        onClick={() => onSelectSession(s.id)}
                    >
                        <div className="truncate">{s.name}</div>
                        <div className="text-xs text-gray-400">{s.messageCount} messages</div>
                    </div>
                ))}
            </div>
            <div className="border-t border-gray-700 p-2">
                {navItems.map(({ view, label, icon }) => (
                    <div
                        key={view}
                        className={`px-4 py-2 cursor-pointer text-sm rounded ${
                            activeView === view
                                ? 'bg-gray-700 text-blue-400'
                                : 'text-gray-400 hover:text-white'
                        }`}
                        onClick={() => onNavigate(view)}
                    >
                        {icon} {label}
                    </div>
                ))}
            </div>
        </div>
    );
}
