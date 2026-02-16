import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";
import { LogViewer } from "./components/LogViewer";
import { ChatInterface } from "./components/ChatInterface";

function App() {
  const [appInfo, setAppInfo] = useState<{ version: string, backend_url: string } | null>(null);

  useEffect(() => {
    invoke("get_app_info").then((info: any) => setAppInfo(info));
  }, []);

  return (
    <div className="flex h-screen w-screen bg-slate-950 text-slate-200 overflow-hidden">
      {/* Sidebar / Chat Area */}
      <div className="w-1/3 flex flex-col border-r border-slate-800 p-4">
        <h1 className="text-xl font-bold mb-4 flex items-center gap-2">
          <img src="/tauri.svg" className="w-6 h-6" alt="Logo" />
          Sovereign Claw <span className="text-xs font-normal text-slate-500">v{appInfo?.version}</span>
        </h1>

        <div className="flex-1 overflow-hidden">
          <ChatInterface />
        </div>
      </div>

      {/* Main Content / "Fog of War" Dashboard */}
      <div className="flex-1 flex flex-col p-4 bg-slate-950">
        <LogViewer />
      </div>
    </div>
  );
}

export default App;


