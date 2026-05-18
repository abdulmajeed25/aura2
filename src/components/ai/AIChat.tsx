"use client";

import { useState } from "react";
import { Brain, RefreshCw, Send, Sparkles } from "lucide-react";
import { graphRagQuery, rebuildGraphRag } from "@/lib/tauri/graphRag";
import { useEditorStore } from "@/lib/store/editorStore";
import type { GraphRagAnswer, GraphRagRebuildReport } from "@/types/vault";

export function AIChat() {
  const [question, setQuestion] = useState("");
  const [answer, setAnswer] = useState<GraphRagAnswer | null>(null);
  const [loading, setLoading] = useState(false);
  const [rebuilding, setRebuilding] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [rebuildReport, setRebuildReport] = useState<GraphRagRebuildReport | null>(null);
  const openFile = useEditorStore((s) => s.openFile);

  const submit = async () => {
    const q = question.trim();
    if (!q) return;
    setLoading(true);
    setError(null);
    try {
      const a = await graphRagQuery(q, 3);
      setAnswer(a);
    } catch (e) {
      setError(messageOf(e));
    } finally {
      setLoading(false);
    }
  };

  const rebuild = async () => {
    setRebuilding(true);
    setError(null);
    try {
      const report = await rebuildGraphRag();
      setRebuildReport(report);
    } catch (e) {
      setError(messageOf(e));
    } finally {
      setRebuilding(false);
    }
  };

  return (
    <div className="h-full w-full flex flex-col bg-[var(--color-bg)]">
      <div className="px-4 py-3 border-b border-[var(--color-border)] flex items-center gap-3 text-xs">
        <Brain size={14} className="text-[var(--color-accent)]" />
        <span className="font-medium text-[var(--color-text)]">Global Query (GraphRAG)</span>
        <span className="text-[var(--color-text-faint)]">
          Communities detected by label propagation · summaries embedded for fast retrieval.
        </span>
        <button
          type="button"
          onClick={() => void rebuild()}
          disabled={rebuilding}
          className="ml-auto inline-flex items-center gap-1 px-2 py-1 rounded border border-[var(--color-border)] hover:bg-[var(--color-surface-hover)] disabled:opacity-50"
          title="Rebuild community index"
        >
          <RefreshCw size={12} className={rebuilding ? "animate-spin" : ""} />
          {rebuilding ? "Rebuilding…" : "Rebuild index"}
        </button>
      </div>

      <div className="flex-1 min-h-0 overflow-auto px-6 py-4">
        {!answer && !error && (
          <div className="text-[var(--color-text-faint)] text-sm max-w-xl mx-auto">
            <p>
              Ask a question about your whole vault. Aura retrieves the most
              relevant <em>communities</em> of notes (clusters in your link
              graph) and returns their summaries as a compact context payload.
            </p>
            <p className="mt-3 text-[12px]">
              Example: <em>“What are the patterns in my thinking about productivity?”</em>
            </p>
            {rebuildReport && (
              <p className="mt-4 text-[11px] text-[var(--color-text-faint)]">
                Index ready: {rebuildReport.communities} communities,{" "}
                {rebuildReport.members_total} notes (avg{" "}
                {rebuildReport.avg_members.toFixed(1)} notes/community).
              </p>
            )}
          </div>
        )}
        {error && <div className="text-red-400 text-sm">{error}</div>}
        {answer && <AnswerView answer={answer} onOpenFile={(p) => void openFile(p)} />}
      </div>

      <div className="px-4 py-3 border-t border-[var(--color-border)] flex items-center gap-2">
        <input
          value={question}
          onChange={(e) => setQuestion(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              void submit();
            }
          }}
          placeholder="Ask about your vault…"
          className="flex-1 bg-[var(--color-surface)] border border-[var(--color-border)] rounded px-3 py-2 text-sm text-[var(--color-text)] outline-none focus:border-[var(--color-accent)] placeholder:text-[var(--color-text-faint)]"
        />
        <button
          type="button"
          onClick={() => void submit()}
          disabled={loading || !question.trim()}
          className="inline-flex items-center gap-1 px-3 py-2 rounded bg-[var(--color-accent)] text-[var(--color-bg)] hover:bg-[var(--color-accent-hover)] disabled:opacity-50 text-sm"
        >
          <Send size={13} />
          {loading ? "Querying…" : "Ask"}
        </button>
      </div>
    </div>
  );
}

function AnswerView({
  answer,
  onOpenFile,
}: {
  answer: GraphRagAnswer;
  onOpenFile: (path: string) => void;
}) {
  if (answer.communities.length === 0) {
    return (
      <div className="text-[var(--color-text-faint)] text-sm">
        <p>No matching communities. Try “Rebuild index” first if you haven't yet.</p>
      </div>
    );
  }
  return (
    <div className="max-w-2xl mx-auto space-y-5">
      <div className="text-[11px] text-[var(--color-text-faint)] flex gap-4">
        <span>≈ {answer.estimated_tokens} tokens of context</span>
        <span>{answer.covered_notes} notes covered</span>
      </div>
      {answer.communities.map((c, i) => (
        <article
          key={c.community_id}
          className="border border-[var(--color-border)] rounded-lg overflow-hidden"
        >
          <header className="px-4 py-2 bg-[var(--color-surface)] flex items-center gap-2 text-[12px]">
            <Sparkles size={12} className="text-[var(--color-accent)]" />
            <span className="text-[var(--color-text)] font-medium">
              Theme {i + 1}
            </span>
            <span className="text-[var(--color-text-faint)]">
              {c.member_count} {c.member_count === 1 ? "note" : "notes"}
            </span>
            <span className="ml-auto text-[10px] text-[var(--color-text-faint)] tabular-nums">
              score {c.score.toFixed(3)}
            </span>
          </header>
          <div className="px-4 py-3 text-[13px] text-[var(--color-text-dim)] whitespace-pre-wrap">
            {c.summary_text}
          </div>
          {c.member_paths.length > 0 && (
            <div className="px-4 pb-3 flex flex-wrap gap-2">
              {c.member_paths.map((path, idx) => (
                <button
                  key={path}
                  type="button"
                  onClick={() => onOpenFile(path)}
                  className="text-[11px] px-2 py-1 rounded bg-[var(--color-surface)] border border-[var(--color-border)] hover:border-[var(--color-accent)] text-[var(--color-text-dim)] hover:text-[var(--color-text)]"
                  title={path}
                >
                  {c.member_titles[idx] || path}
                </button>
              ))}
            </div>
          )}
        </article>
      ))}
    </div>
  );
}

function messageOf(e: unknown): string {
  if (e && typeof e === "object" && "message" in e) {
    return String((e as { message: unknown }).message);
  }
  return String(e);
}
