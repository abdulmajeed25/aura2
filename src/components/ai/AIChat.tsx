"use client";

import { useEffect, useState } from "react";
import { Brain, RefreshCw, RotateCcw, Send, Sparkles, Waves } from "lucide-react";
import { graphRagQuery, rebuildGraphRag } from "@/lib/tauri/graphRag";
import { ssmReset, ssmStatus, streamingChat } from "@/lib/tauri/streaming";
import { useEditorStore } from "@/lib/store/editorStore";
import type {
  GraphRagAnswer,
  GraphRagRebuildReport,
  SsmStatus,
} from "@/types/vault";

interface Turn {
  question: string;
  answer: GraphRagAnswer;
}

export function AIChat() {
  const [question, setQuestion] = useState("");
  const [turns, setTurns] = useState<Turn[]>([]);
  const [loading, setLoading] = useState(false);
  const [rebuilding, setRebuilding] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [rebuildReport, setRebuildReport] = useState<GraphRagRebuildReport | null>(null);
  const [continuous, setContinuous] = useState(false);
  const [ssm, setSsm] = useState<SsmStatus | null>(null);
  const openFile = useEditorStore((s) => s.openFile);

  useEffect(() => {
    void ssmStatus().then(setSsm).catch(() => {});
  }, []);

  const submit = async () => {
    const q = question.trim();
    if (!q) return;
    setLoading(true);
    setError(null);
    try {
      if (continuous) {
        const turn = await streamingChat(q, 0.5, 3);
        setTurns((t) => [...t, { question: q, answer: turn.answer }]);
        setSsm(turn.status);
      } else {
        const ans = await graphRagQuery(q, 3);
        setTurns((t) => [...t, { question: q, answer: ans }]);
      }
      setQuestion("");
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
      setRebuildReport(await rebuildGraphRag());
    } catch (e) {
      setError(messageOf(e));
    } finally {
      setRebuilding(false);
    }
  };

  const resetState = async () => {
    try {
      setSsm(await ssmReset());
      setTurns([]);
    } catch (e) {
      setError(messageOf(e));
    }
  };

  return (
    <div className="h-full w-full flex flex-col bg-[var(--color-bg)]">
      <div className="px-4 py-3 border-b border-[var(--color-border)] flex items-center gap-3 text-xs flex-wrap">
        <Brain size={14} className="text-[var(--color-accent)]" />
        <span className="font-medium text-[var(--color-text)]">Global Query (GraphRAG)</span>
        <ContinuousToggle on={continuous} onChange={setContinuous} />
        {continuous && ssm && <SsmIndicator status={ssm} />}
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
        {continuous && (
          <button
            type="button"
            onClick={() => void resetState()}
            className="inline-flex items-center gap-1 px-2 py-1 rounded border border-[var(--color-border)] hover:bg-[var(--color-surface-hover)]"
            title="Reset streaming state and transcript"
          >
            <RotateCcw size={12} />
            Reset state
          </button>
        )}
      </div>

      <div className="flex-1 min-h-0 overflow-auto px-6 py-4">
        {turns.length === 0 && !error && (
          <div className="text-[var(--color-text-faint)] text-sm max-w-xl mx-auto">
            <p>
              Ask a question about your whole vault. Aura retrieves the most
              relevant <em>communities</em> of notes and returns their
              summaries as a compact context payload.
            </p>
            <p className="mt-3 text-[12px]">
              Example: <em>“What are the patterns in my thinking about productivity?”</em>
            </p>
            <p className="mt-3 text-[12px]">
              Enable <strong>Continuous</strong> to keep a fixed-size streaming
              state across turns so retrieval is biased by the conversation,
              not just the latest message.
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
        <div className="max-w-2xl mx-auto space-y-7">
          {turns.map((turn, i) => (
            <TurnView
              key={i}
              turn={turn}
              onOpenFile={(p) => void openFile(p)}
            />
          ))}
        </div>
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
          placeholder={continuous ? "Continue the conversation…" : "Ask about your vault…"}
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

function ContinuousToggle({
  on,
  onChange,
}: {
  on: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <button
      type="button"
      onClick={() => onChange(!on)}
      className={
        "inline-flex items-center gap-1.5 px-2 py-1 rounded border text-[11px] " +
        (on
          ? "border-[var(--color-accent)] text-[var(--color-accent)] bg-[var(--color-surface-hover)]"
          : "border-[var(--color-border)] text-[var(--color-text-faint)] hover:text-[var(--color-text)]")
      }
      title="Stream state across turns (Mamba-style SSM)"
    >
      <Waves size={11} />
      Continuous {on ? "on" : "off"}
    </button>
  );
}

function SsmIndicator({ status }: { status: SsmStatus }) {
  const pct = Math.min(100, Math.max(0, Math.round(status.saturation * 100)));
  return (
    <div className="inline-flex items-center gap-2 text-[11px] text-[var(--color-text-faint)]">
      <span>steps {status.step_count}</span>
      <div
        className="h-2 w-24 rounded-full bg-[var(--color-surface)] overflow-hidden"
        title="State saturation"
      >
        <div
          className="h-full bg-[var(--color-accent)]"
          style={{ width: `${pct}%` }}
        />
      </div>
      <span>align {status.last_input_alignment.toFixed(2)}</span>
    </div>
  );
}

function TurnView({
  turn,
  onOpenFile,
}: {
  turn: Turn;
  onOpenFile: (path: string) => void;
}) {
  const a = turn.answer;
  return (
    <div>
      <div className="flex gap-2 text-[12px] text-[var(--color-text-dim)] mb-2">
        <span className="text-[var(--color-text)] font-medium">You:</span>
        <span>{turn.question}</span>
      </div>
      {a.communities.length === 0 ? (
        <p className="text-[var(--color-text-faint)] text-[12px] pl-4">
          No matching communities. Try “Rebuild index” first.
        </p>
      ) : (
        <div className="space-y-3 pl-4 border-l-2 border-[var(--color-border)]">
          <div className="text-[11px] text-[var(--color-text-faint)] flex gap-4">
            <span>≈ {a.estimated_tokens} tokens of context</span>
            <span>{a.covered_notes} notes covered</span>
          </div>
          {a.communities.map((c, i) => (
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
      )}
    </div>
  );
}

function messageOf(e: unknown): string {
  if (e && typeof e === "object" && "message" in e) {
    return String((e as { message: unknown }).message);
  }
  return String(e);
}
