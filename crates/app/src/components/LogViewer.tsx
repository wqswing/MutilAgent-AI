import { useEffect, useState, useRef } from "react";

interface LogEntry {
    timestamp: string;
    level: string;
    message: string;
    target: string;
    [key: string]: any;
}

export function LogViewer() {
    const [logs, setLogs] = useState<LogEntry[]>([]);
    const [connected, setConnected] = useState(false);
    const bottomRef = useRef<HTMLDivElement>(null);
    const [autoScroll, setAutoScroll] = useState(true);

    useEffect(() => {
        // Retry connection logic
        let socket: WebSocket | null = null;
        let retryTimeout: any = null;

        const connect = () => {
            socket = new WebSocket("ws://127.0.0.1:3000/ws/logs");

            socket.onopen = () => {
                setConnected(true);
                console.log("Connected to Log Stream");
            };

            socket.onmessage = (event) => {
                try {
                    const data = JSON.parse(event.data);
                    setLogs((prev) => [...prev.slice(-999), data]); // Keep last 1000 logs
                } catch (e) {
                    console.error("Failed to parse log:", event.data);
                }
            };

            socket.onclose = () => {
                setConnected(false);
                retryTimeout = setTimeout(connect, 3000);
            };

            socket.onerror = (err) => {
                console.error("WebSocket error:", err);
                socket?.close();
            };
        };

        connect();

        return () => {
            if (socket) socket.close();
            if (retryTimeout) clearTimeout(retryTimeout);
        };
    }, []);

    useEffect(() => {
        if (autoScroll && bottomRef.current) {
            bottomRef.current.scrollIntoView({ behavior: "smooth" });
        }
    }, [logs, autoScroll]);

    return (
        <div className="flex flex-col h-full bg-slate-950 text-slate-200 font-mono text-xs rounded-lg border border-slate-800 overflow-hidden shadow-xl">
            <div className="flex items-center justify-between px-4 py-2 bg-slate-900 border-b border-slate-800">
                <div className="flex items-center gap-2">
                    <div className={`w-2 h-2 rounded-full ${connected ? "bg-green-500 animate-pulse" : "bg-red-500"}`} />
                    <span className="font-semibold text-slate-400">Fog of War</span>
                </div>
                <div className="flex items-center gap-2">
                    <label className="flex items-center gap-1 cursor-pointer text-xs text-slate-500 hover:text-slate-300">
                        <input type="checkbox" checked={autoScroll} onChange={(e) => setAutoScroll(e.target.checked)} />
                        Auto-scroll
                    </label>
                    <button onClick={() => setLogs([])} className="hover:text-white transition-colors">Clear</button>
                </div>
            </div>

            <div className="flex-1 overflow-y-auto p-4 space-y-1">
                {logs.length === 0 && (
                    <div className="text-slate-600 text-center mt-10 italic">Waiting for agents...</div>
                )}
                {logs.map((log, i) => (
                    <div key={i} className="flex gap-2 break-all hover:bg-slate-900/50 p-0.5 rounded">
                        <span className="text-slate-600 shrink-0 w-24">{new Date(log.timestamp).toLocaleTimeString()}</span>
                        <span className={`shrink-0 w-16 font-bold ${getLevelColor(log.level)}`}>{log.level}</span>
                        <span className="text-slate-500 shrink-0 w-32">{log.target}</span>
                        <span className="text-slate-300">{log.message} {renderExtras(log)}</span>
                    </div>
                ))}
                <div ref={bottomRef} />
            </div>
        </div>
    );
}

function getLevelColor(level: string) {
    switch (level) {
        case "INFO": return "text-blue-400";
        case "WARN": return "text-yellow-400";
        case "ERROR": return "text-red-500";
        case "DEBUG": return "text-purple-400";
        default: return "text-slate-400";
    }
}

function renderExtras(log: LogEntry) {
    const extras = Object.entries(log).filter(([k]) => !["timestamp", "level", "message", "target"].includes(k));
    if (extras.length === 0) return null;
    return (
        <span className="text-slate-500 ml-2 text-xs">
            {extras.map(([k, v]) => `${k}=${JSON.stringify(v)}`).join(" ")}
        </span>
    );
}
