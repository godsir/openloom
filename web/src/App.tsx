import React, { useState, useEffect } from 'react';
import Sidebar from './components/Sidebar';
import ChatArea from './components/ChatArea';
import SettingsPanel from './components/SettingsPanel';
import TokenDashboard from './components/TokenDashboard';
import CognitionAuditPanel from './components/CognitionAuditPanel';

type View = 'chat' | 'settings' | 'dashboard' | 'persona';

interface Session {
    id: string;
    name: string;
    messageCount: number;
}

export default function App() {
    const [activeView, setActiveView] = useState<View>('chat');
    const [sessions, setSessions] = useState<Session[]>([]);
    const [activeSession, setActiveSession] = useState('');
    const [loading, setLoading] = useState(true);

    useEffect(() => {
        window.openloom?.send('session.list').then((data: any) => {
            const list = (data.sessions || []) as Session[];
            if (list.length > 0) {
                setSessions(list);
                setActiveSession(list[0].id);
            } else {
                window.openloom?.send('session.create').then((s: any) => {
                    const newSession = { id: s.id, name: 'Default', messageCount: 0 };
                    setSessions([newSession]);
                    setActiveSession(s.id);
                });
            }
        }).catch(() => {
            setSessions([{ id: 'default', name: 'Default (offline)', messageCount: 0 }]);
            setActiveSession('default');
        }).finally(() => setLoading(false));
    }, []);

    const handleNewSession = async () => {
        try {
            const s: any = await window.openloom?.send('session.create');
            setSessions(prev => [...prev, { id: s.id, name: `Session ${prev.length + 1}`, messageCount: 0 }]);
            setActiveSession(s.id);
        } catch {
            const id = crypto.randomUUID();
            setSessions(prev => [...prev, { id, name: `Session ${prev.length + 1}`, messageCount: 0 }]);
            setActiveSession(id);
        }
    };

    const handleSwitchSession = async (id: string) => {
        try {
            await window.openloom?.send('session.switch', { session_id: id });
        } catch {}
        setActiveSession(id);
    };

    if (loading) {
        return <div className="flex h-screen items-center justify-center text-gray-400">Connecting to engine...</div>;
    }

    return (
        <div className="flex h-screen">
            <Sidebar
                sessions={sessions}
                activeSession={activeSession}
                onSelectSession={handleSwitchSession}
                onNewSession={handleNewSession}
                onNavigate={setActiveView}
                activeView={activeView}
            />
            <main className="flex-1 flex flex-col">
                {activeView === 'chat' && <ChatArea sessionId={activeSession} />}
                {activeView === 'settings' && <SettingsPanel />}
                {activeView === 'dashboard' && <TokenDashboard />}
                {activeView === 'persona' && <CognitionAuditPanel />}
            </main>
        </div>
    );
}
