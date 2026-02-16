import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

interface CheckResult {
    category: string;
    name: string;
    status: "pass" | "fail" | "warn";
    message?: string;
    latency_ms?: number;
}

interface DoctorReport {
    overall_status: "healthy" | "degraded" | "down";
    checks: CheckResult[];
}

export function SystemHealth() {
    const [report, setReport] = useState<DoctorReport | null>(null);
    const [loading, setLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [backendUrl, setBackendUrl] = useState<string | null>(null);

    useEffect(() => {
        invoke("get_app_info").then((info: any) => {
            setBackendUrl(info.backend_url);
            fetchReport(info.backend_url);
        });
    }, []);

    const fetchReport = async (url: string) => {
        setLoading(true);
        setError(null);
        try {
            const response = await fetch(`${url}/admin/doctor`, {
                method: "POST",
                headers: {
                    // "Authorization": "Bearer ...", // If we need auth later
                }
            });
            if (!response.ok) {
                throw new Error(`HTTP error! status: ${response.status}`);
            }
            const data = await response.json();
            setReport(data);
        } catch (e: any) {
            setError(e.message);
        } finally {
            setLoading(false);
        }
    };

    const getStatusColor = (status: string) => {
        switch (status) {
            case "pass":
            case "healthy":
                return "text-green-400";
            case "warn":
            case "degraded":
                return "text-yellow-400";
            case "fail":
            case "down":
                return "text-red-400";
            default:
                return "text-slate-400";
        }
    };

    return (
        <div className="flex flex-col h-full bg-slate-900 rounded-lg p-6 overflow-hidden">
            <div className="flex justify-between items-center mb-6">
                <h2 className="text-2xl font-bold flex items-center gap-3">
                    🩺 System Health
                    {report && (
                        <span className={`text-sm px-3 py-1 rounded-full bg-slate-800 border ${getStatusColor(report.overall_status)} border-current`}>
                            {report.overall_status.toUpperCase()}
                        </span>
                    )}
                </h2>
                <button
                    onClick={() => backendUrl && fetchReport(backendUrl)}
                    disabled={loading}
                    className="px-4 py-2 bg-slate-700 hover:bg-slate-600 rounded text-sm transition-colors disabled:opacity-50"
                >
                    {loading ? "Diagnosing..." : "Run Diagnostics"}
                </button>
            </div>

            {error && (
                <div className="p-4 bg-red-900/20 border border-red-500/50 rounded-lg text-red-200 mb-6">
                    Error running diagnostics: {error}
                </div>
            )}

            <div className="flex-1 overflow-auto pr-2">
                {report ? (
                    <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                        {report.checks.map((check, idx) => (
                            <div key={idx} className="bg-slate-800/50 p-4 rounded-lg border border-slate-700 hover:border-slate-600 transition-colors">
                                <div className="flex justify-between items-start mb-2">
                                    <div className="flex flex-col">
                                        <span className="text-xs text-slate-500 uppercase tracking-wider">{check.category}</span>
                                        <span className="font-semibold">{check.name}</span>
                                    </div>
                                    <div className={`text-xs font-bold px-2 py-1 rounded bg-slate-900 ${getStatusColor(check.status)}`}>
                                        {check.status.toUpperCase()}
                                    </div>
                                </div>

                                {check.message && (
                                    <div className="text-sm text-slate-400 mt-2 bg-slate-900/50 p-2 rounded">
                                        {check.message}
                                    </div>
                                )}

                                {check.latency_ms !== undefined && (
                                    <div className="mt-3 flex items-center gap-2 text-xs text-slate-500">
                                        <svg className="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" />
                                        </svg>
                                        {check.latency_ms}ms
                                    </div>
                                )}
                            </div>
                        ))}
                    </div>
                ) : (
                    !loading && !error && (
                        <div className="flex flex-col items-center justify-center h-64 text-slate-500">
                            <p>No diagnostics data available.</p>
                            <p className="text-sm">Click "Run Diagnostics" to check system health.</p>
                        </div>
                    )
                )}
            </div>
        </div>
    );
}
