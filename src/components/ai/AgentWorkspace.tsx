"use client";

import { useEffect, useState } from "react";
import { Bot, CheckCircle2, GitFork, Loader2, Workflow } from "lucide-react";
import {
  applyLinkSuggestion,
  findOrphanNotes,
  suggestLinks,
} from "@/lib/tauri/agent";
import { useEditorStore } from "@/lib/store/editorStore";
import type { LinkSuggestion, OrphanNote } from "@/types/vault";

type Tab = "suggestions" | "orphans";

export function AgentWorkspace() {
  const [tab, setTab] = useState<Tab>("suggestions");
  const [suggestions, setSuggestions] = useState<LinkSuggestion[]>([]);
  const [orphans, setOrphans] = useState<OrphanNote[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [applied, setApplied] = useState<Set<string>>(new Set());
  const openFile = useEditorStore((s) => s.openFile);

  const refreshSuggestions = async () => {
    setLoading(true);
    setError(null);
    try {
      setSuggestions(await suggestLinks({ totalLimit: 60 }));
    } catch (e) {
      setError(messageOf(e));
    } finally {
      setLoading(false);
    }
  };

  const refreshOrphans = async () => {
    setLoading(true);
    setError(null);
    try {
      setOrphans(await findOrphanNotes());
    } catch (e) {
      setError(messageOf(e));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (tab === "suggestions") {
      void refreshSuggestions();
    } else {
      void refreshOrphans();
    }
  }, [tab]);

  const applyOne = async (s: LinkSuggestion) => {
    const key = `${s.source_file_id}->${s.target_file_id}`;
    setError(null);
    try {
      await applyLinkSuggestion(s.source_path, s.target_path);
      setApplied((prev) => new Set(prev).add(key));
    } catch (e) {
      setError(messageOf(e));
    }
  };

  return (
    <div className="h-full w-full flex flex-col bg-[var(--color-bg)]">
      <header className="px-4 py-3 border-b border-[var(--color-border)] flex items-center gap-3 text-xs">
        <Bot size={14} className="text-[var(--color-accent)]" />
        <span className="font-medium text-[var(--color-text)]">
          Agent workspace
        </span>
        <span className="text-[var(--color-text-faint)]">
          Review proposals; nothing is changed until you click Apply.
        </span>
        <div className="ml-auto flex items-center gap-1 rounded border border-[var(--color-border)] p-0.5">
          <TabButton
            active={tab === "suggestions"}
            onClick={() => setTab("suggestions")}
            icon={<GitFork size={11} />}
            label="Suggested links"
            count={suggestions.length}
          />
          <TabButton
            active={tab === "orphans"}
            onClick={() => setTab("orphans")}
            icon={<Workflow size={11} />}
            label="Orphans"
            count={orphans.length}
          />
        </div>
      </header>

      <div className="flex-1 min-h-0 overflow-auto px-6 py-4">
        {loading && (
          <div className="flex items-center gap-2 text-[var(--color-text-faint)] text-[12px]">
            <Loader2 size={12} className="animate-spin" />
            Computing…
          </div>
        )}
        {error && (
          <div className="rounded border border-red-700 bg-red-950/40 px-3 py-2 text-red-300 text-[12px] mb-3">
            {error}
          </div>
        )}

        {tab === "suggestions" && !loading && (
          <SuggestionsList
            items={suggestions}
            applied={applied}
            onApply={applyOne}
            onOpen={openFile}
          />
        )}
        {tab === "orphans" && !loading && (
          <OrphansList items={orphans} onOpen={openFile} />
        )}
      </div>
    </div>
  );
}

function TabButton({
  active,
  onClick,
  icon,
  label,
  count,
}: {
  active: boolean;
  onClick: () => void;
  icon: React.ReactNode;
  label: string;
  count: number;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={
        "inline-flex items-center gap-1 px-2 py-1 rounded text-[11px] " +
        (active
          ? "bg-[var(--color-surface-hover)] text-[var(--color-accent)]"
          : "text-[var(--color-text-faint)] hover:text-[var(--color-text)]")
      }
    >
      {icon}
      {label}
      <span className="text-[10px] text-[var(--color-text-faint)]">
        {count}
      </span>
    </button>
  );
}

function SuggestionsList({
  items,
  applied,
  onApply,
  onOpen,
}: {
  items: LinkSuggestion[];
  applied: Set<string>;
  onApply: (s: LinkSuggestion) => Promise<void>;
  onOpen: (p: string) => void;
}) {
  if (items.length === 0) {
    return (
      <p className="text-[var(--color-text-faint)] text-[12px] max-w-xl">
        No link suggestions yet. This usually means either the vault is too
        small or HDC similarity didn't clear the threshold. Try reducing the
        score floor on the backend.
      </p>
    );
  }
  return (
    <ul className="max-w-3xl mx-auto space-y-2">
      {items.map((s) => {
        const key = `${s.source_file_id}->${s.target_file_id}`;
        const done = applied.has(key);
        return (
          <li
            key={key}
            className="border border-[var(--color-border)] rounded-lg overflow-hidden"
          >
            <div className="px-4 py-3 flex items-center gap-3 text-[12px]">
              <button
                type="button"
                onClick={() => onOpen(s.source_path)}
                className="text-[var(--color-text)] hover:text-[var(--color-accent)] underline-offset-2 hover:underline truncate"
              >
                {s.source_title}
              </button>
              <span className="text-[var(--color-text-faint)]">→</span>
              <button
                type="button"
                onClick={() => onOpen(s.target_path)}
                className="text-[var(--color-text)] hover:text-[var(--color-accent)] underline-offset-2 hover:underline truncate"
              >
                {s.target_title}
              </button>
              <span className="ml-auto flex items-center gap-3 text-[10px] text-[var(--color-text-faint)] tabular-nums">
                <span>score {s.score.toFixed(3)}</span>
                {s.shared_neighbours > 0 && (
                  <span title="Shared neighbours">⇄ {s.shared_neighbours}</span>
                )}
                {done ? (
                  <span className="inline-flex items-center gap-1 text-emerald-400">
                    <CheckCircle2 size={11} />
                    Applied
                  </span>
                ) : (
                  <button
                    type="button"
                    onClick={() => void onApply(s)}
                    className="px-2 py-0.5 rounded bg-[var(--color-accent)] text-[var(--color-bg)] hover:bg-[var(--color-accent-hover)] text-[11px]"
                  >
                    Apply
                  </button>
                )}
              </span>
            </div>
            <div className="px-4 pb-2 text-[10px] text-[var(--color-text-faint)] truncate">
              {s.source_path} → {s.target_path}
            </div>
          </li>
        );
      })}
    </ul>
  );
}

function OrphansList({
  items,
  onOpen,
}: {
  items: OrphanNote[];
  onOpen: (p: string) => void;
}) {
  if (items.length === 0) {
    return (
      <p className="text-[var(--color-text-faint)] text-[12px] max-w-xl">
        No orphans — every note in the vault has at least one resolved link.
      </p>
    );
  }
  return (
    <ul className="max-w-3xl mx-auto space-y-1">
      {items.map((o) => (
        <li
          key={o.file_id}
          className="flex items-center gap-3 px-3 py-2 border border-[var(--color-border)] rounded text-[12px]"
        >
          <button
            type="button"
            onClick={() => onOpen(o.path)}
            className="text-[var(--color-text)] hover:text-[var(--color-accent)] truncate flex-1 text-left"
          >
            {o.title}
          </button>
          <span className="text-[var(--color-text-faint)] truncate">
            {o.path}
          </span>
        </li>
      ))}
    </ul>
  );
}

function messageOf(e: unknown): string {
  if (e && typeof e === "object" && "message" in e) {
    return String((e as { message: unknown }).message);
  }
  return String(e);
}
