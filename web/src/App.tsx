import React, { useState } from 'react';
import Sidebar from './components/Sidebar';
import ChatArea from './components/ChatArea';
import SettingsPanel from './components/SettingsPanel';
import TokenDashboard from './components/TokenDashboard';

type View = 'chat' | 'settings' | 'dashboard';

interface Session {
    id: string;
    name: string;
    messageCount: number;
}

export default function App() {
    const [activeView, setActiveView] = useState<View>('chat');
    const [sessions, setSessions] = useState<Session[]>([
        { id: 'default', name: 'Default Session', messageCount: 0 },
    ]);
    const [activeSession, setActiveSession] = useState('default');

    const handleNewSession = () => {
        const id = crypto.randomUUID();
        setSessions([
            ...sessions,
            { id, name: `Session ${sessions.length + 1}`, messageCount: 0 },
        ]);
        setActiveSession(id);
    };

    return (
        <div className="flex h-screen">
            <Sidebar
                sessions={sessions}
                activeSession={activeSession}
                onSelectSession={setActiveSession}
                onNewSession={handleNewSession}
                onNavigate={setActiveView}
                activeView={activeView}
            />
            <main className="flex-1 flex flex-col">
                {activeView === 'chat' && <ChatArea sessionId={activeSession} />}
                {activeView === 'settings' && <SettingsPanel />}
                {activeView === 'dashboard' && <TokenDashboard />}
            </main>
        </div>
    );
}
