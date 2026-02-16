import { useState, useRef, useEffect } from "react";
// import { invoke } from "@tauri-apps/api/core";

interface Message {
    role: "user" | "assistant";
    content: string;
}

export function ChatInterface() {
    const [messages, setMessages] = useState<Message[]>([
        { role: "assistant", content: "Hello! I am Sovereign Claw. How can I help you today?" }
    ]);
    const [input, setInput] = useState("");
    const [loading, setLoading] = useState(false);
    const scrollRef = useRef<HTMLDivElement>(null);

    useEffect(() => {
        if (scrollRef.current) {
            scrollRef.current.scrollIntoView({ behavior: "smooth" });
        }
    }, [messages]);

    const handleSubmit = async (e: React.FormEvent) => {
        e.preventDefault();
        if (!input.trim() || loading) return;

        const userMsg: Message = { role: "user", content: input };
        setMessages(prev => [...prev, userMsg]);
        setInput("");
        setLoading(true);

        try {
            // TODO: Implement actual backend connection
            // const response = await invoke("chat", { message: input });

            // Simulate response for now
            setTimeout(() => {
                setMessages(prev => [...prev, {
                    role: "assistant",
                    content: "I received your message but I am currently in 'Fog of War' mode. My actual reasoning capabilities are being integrated."
                }]);
                setLoading(false);
            }, 1000);
        } catch (err) {
            console.error("Chat error:", err);
            setLoading(false);
        }
    };

    return (
        <div className="flex flex-col h-full bg-slate-900 rounded-lg border border-slate-800 overflow-hidden">
            <div className="p-4 border-b border-slate-800 bg-slate-900/50">
                <h2 className="text-sm font-semibold text-slate-300">Chat Session</h2>
            </div>

            <div className="flex-1 overflow-y-auto p-4 space-y-4">
                {messages.map((msg, i) => (
                    <div key={i} className={`flex ${msg.role === "user" ? "justify-end" : "justify-start"}`}>
                        <div className={`max-w-[80%] rounded-lg px-4 py-2 text-sm ${msg.role === "user"
                                ? "bg-blue-600 text-white"
                                : "bg-slate-800 text-slate-200 border border-slate-700"
                            }`}>
                            {msg.content}
                        </div>
                    </div>
                ))}
                {loading && (
                    <div className="flex justify-start">
                        <div className="bg-slate-800 text-slate-400 rounded-lg px-4 py-2 text-sm border border-slate-700 animate-pulse">
                            Thinking...
                        </div>
                    </div>
                )}
                <div ref={scrollRef} />
            </div>

            <form onSubmit={handleSubmit} className="p-4 border-t border-slate-800 bg-slate-900">
                <div className="flex gap-2">
                    <input
                        className="flex-1 bg-slate-950 border border-slate-700 rounded-lg px-3 py-2 text-sm focus:outline-none focus:border-blue-500 transition-colors"
                        placeholder="Type a message..."
                        value={input}
                        onChange={(e) => setInput(e.target.value)}
                        disabled={loading}
                    />
                    <button
                        type="submit"
                        disabled={loading || !input.trim()}
                        className="bg-blue-600 hover:bg-blue-700 disabled:opacity-50 disabled:cursor-not-allowed text-white px-4 py-2 rounded-lg text-sm font-medium transition-colors"
                    >
                        Send
                    </button>
                </div>
            </form>
        </div>
    );
}
