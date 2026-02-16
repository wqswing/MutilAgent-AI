import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

interface AuditEntry {
    id: string;
    timestamp: string;
    user_id: string;
    action: string;
    resource: string;
    outcome: "Success" | "Denied" | { Error: string };
    metadata?: any;
    previous_hash?: string;
    hash?: string;
}



export function AuditViewer() {
    const [entries, setEntries] = useState<AuditEntry[]>([]);
    const [loading, setLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [backendUrl, setBackendUrl] = useState<string | null>(null);

    // Filters
    const [userId, setUserId] = useState("");
    const [action, setAction] = useState("");
    const [resource, setResource] = useState("");

    useEffect(() => {
        invoke("get_app_info").then((info: any) => {
            setBackendUrl(info.backend_url);
            fetchAuditLogs(info.backend_url);
        });
    }, []);

    const fetchAuditLogs = async (url: string) => {
        setLoading(true);
        setError(null);
        try {
            const params = new URLSearchParams();
            if (userId) params.append("user_id", userId);
            if (action) params.append("action", action);
            if (resource) params.append("resource", resource);
            params.append("limit", "50");

            const response = await fetch(`${url}/admin/audit?${params.toString()}`);
            if (!response.ok) {
                throw new Error(`HTTP error! status: ${response.status}`);
            }
            const data = await response.json();
            setEntries(data);
        } catch (e: any) {
            setError(e.message);
        } finally {
            setLoading(false);
        }
    };

    const handleSearch = () => {
        if (backendUrl) {
            fetchAuditLogs(backendUrl);
        }
    };

    const getOutcomeIcon = (outcome: any) => {
        if (outcome === "Success") return <span className="text-green-400">✅</span>;
        if (outcome === "Denied") return <span className="text-yellow-400">⛔</span>;
        return <span className="text-red-400" title={JSON.stringify(outcome)}>❌</span>;
    };

    return (
        <div className="flex flex-col h-full bg-slate-900 rounded-lg p-6 overflow-hidden">
            <div className="flex justify-between items-center mb-6">
                <h2 className="text-2xl font-bold flex items-center gap-3">
                    🛡️ Audit Log
                </h2>
                <div className="flex gap-2">
                    <button
                        onClick={handleSearch}
                        disabled={loading}
                        className="px-4 py-2 bg-slate-700 hover:bg-slate-600 rounded text-sm transition-colors disabled:opacity-50"
                    >
                        Refresh
                    </button>
                </div>
            </div>

            {/* Filters */}
            <div className="flex gap-4 mb-4 p-4 bg-slate-800 rounded-lg">
                <input
                    type="text"
                    placeholder="User ID"
                    value={userId}
                    onChange={(e) => setUserId(e.target.value)}
                    className="bg-slate-900 border border-slate-700 rounded px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-blue-500"
                />
                <input
                    type="text"
                    placeholder="Action"
                    value={action}
                    onChange={(e) => setAction(e.target.value)}
                    className="bg-slate-900 border border-slate-700 rounded px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-blue-500"
                />
                <input
                    type="text"
                    placeholder="Resource"
                    value={resource}
                    onChange={(e) => setResource(e.target.value)}
                    className="bg-slate-900 border border-slate-700 rounded px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-blue-500"
                />
                <button
                    onClick={handleSearch}
                    className="px-4 py-2 bg-blue-600 hover:bg-blue-500 rounded text-sm font-medium transition-colors"
                >
                    Filter
                </button>
            </div>

            {error && (
                <div className="p-4 bg-red-900/20 border border-red-500/50 rounded-lg text-red-200 mb-6">
                    Error: {error}
                </div>
            )}

            <div className="flex-1 overflow-auto border border-slate-700 rounded-lg">
                <table className="w-full text-left text-sm text-slate-400">
                    <thead className="text-xs uppercase bg-slate-800 text-slate-200 sticky top-0">
                        <tr>
                            <th className="px-6 py-3">Timestamp</th>
                            <th className="px-6 py-3">User</th>
                            <th className="px-6 py-3">Action</th>
                            <th className="px-6 py-3">Resource</th>
                            <th className="px-6 py-3">Outcome</th>
                            <th className="px-6 py-3">Hash (Short)</th>
                        </tr>
                    </thead>
                    <tbody className="divide-y divide-slate-700">
                        {entries.map((entry) => (
                            <tr key={entry.id} className="bg-slate-900 hover:bg-slate-800 transition-colors">
                                <td className="px-6 py-4 whitespace-nowrap">
                                    {new Date(entry.timestamp).toLocaleString()}
                                </td>
                                <td className="px-6 py-4 font-medium text-slate-200">
                                    {entry.user_id}
                                </td>
                                <td className="px-6 py-4">
                                    <span className="px-2 py-1 bg-slate-800 rounded text-xs border border-slate-700">
                                        {entry.action}
                                    </span>
                                </td>
                                <td className="px-6 py-4 font-mono text-xs text-slate-500">
                                    {entry.resource}
                                </td>
                                <td className="px-6 py-4">
                                    {getOutcomeIcon(entry.outcome)}
                                </td>
                                <td className="px-6 py-4 font-mono text-xs text-slate-600" title={entry.hash}>
                                    {entry.hash?.substring(0, 8)}...
                                </td>
                            </tr>
                        ))}
                        {!loading && entries.length === 0 && (
                            <tr>
                                <td colSpan={6} className="px-6 py-12 text-center text-slate-500">
                                    No audit entries found.
                                </td>
                            </tr>
                        )}
                    </tbody>
                </table>
            </div>
        </div>
    );
}
