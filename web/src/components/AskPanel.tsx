import { useMutation, useQuery } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";
import { api } from "../api";
import { Panel } from "./layout";
import { SdMd } from "./SdMd";

type Turn = { role: "user" | "model"; text: string };

/** "Ask about this page": a short conversation with the configured model about what is being read. Hidden when AI is off. The
 *  conversation lives only on the page. */
export function AskPanel({ kind, id, title = "Ask about this page", hint }: { kind: "sd-concept" | "sd-problem" | "dsa-problem"; id: string; title?: string; hint?: string }) {
    const status = useQuery({ queryKey: ["ai"], queryFn: api.aiStatus, staleTime: 5 * 60_000 });
    const [turns, setTurns] = useState<Turn[]>([]);
    const [text, setText] = useState("");
    const box = useRef<HTMLDivElement>(null);
    useEffect(() => {
        setTurns([]);
    }, [kind, id]);
    // braces matter: scrollTo returns a promise in current browsers, and an effect must return nothing or a cleanup function
    useEffect(() => {
        box.current?.scrollTo({ top: box.current.scrollHeight, behavior: "smooth" });
    }, [turns]);
    const ask = useMutation({
        mutationFn: (next: Turn[]) => api.aiAsk(kind, id, next),
        onSuccess: (r, next) => setTurns([...next, { role: "model", text: r.answer }]),
    });
    if (!status.data?.enabled) return null;
    const send = () => {
        const q = text.trim();
        if (!q || ask.isPending) return;
        const next: Turn[] = [...turns, { role: "user", text: q }];
        setTurns(next);
        setText("");
        ask.mutate(next);
    };
    return (
        <Panel title={title} aside={<span className="ai-model">{status.data.model}</span>} className="ai-pn">
            <div className="ai-log" ref={box}>
                {turns.length === 0 && <p className="ai-hint">{hint ?? "Ask anything about this page: why a choice, what breaks, a hint before the answer."}</p>}
                {turns.map((t, i) => (
                    <div key={i} className={`ai-turn ${t.role}`}>{t.role === "model" ? <SdMd text={t.text} /> : <p>{t.text}</p>}</div>
                ))}
                {ask.isPending && <div className="ai-turn model ai-think" aria-label="Thinking"><i /><i /><i /></div>}
                {ask.isError && <p className="ai-err">{(ask.error as Error).message}</p>}
            </div>
            <form className="ai-ask" onSubmit={(e) => { e.preventDefault(); send(); }}>
                <textarea value={text} onChange={(e) => setText(e.target.value)} onKeyDown={(e) => { if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); send(); } }} placeholder="Ask a question…" rows={2} aria-label={title} />
                <button disabled={!text.trim() || ask.isPending}>Ask</button>
            </form>
        </Panel>
    );
}
