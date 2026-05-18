"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { FileImage, FileVideo, Music, Search, X } from "lucide-react";
import { searchVault } from "@/lib/tauri/search";
import { useEditorStore } from "@/lib/store/editorStore";
import type { SearchHit, SearchMode } from "@/types/vault";

interface Props {
  open: boolean;
  onClose: () => void;
}

export function SearchPalette({ open, onClose }: Props) {
  const [query, setQuery] = useState("");
  const [mode, setMode] = useState<SearchMode>("hybrid");
  const [hits, setHits] = useState<SearchHit[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [focusedIdx, setFocusedIdx] = useState(0);
  const inputRef = useRef<HTMLInputElement | null>(null);
  const queryId = useRef(0);

  // Auto-focus when the palette opens.
  useEffect(() => {
    if (open) {
      setTimeout(() => inputRef.current?.focus(), 0);
    } else {
      setQuery("");
      setHits([]);
      setError(null);
      setFocusedIdx(0);
    }
  }, [open]);

  // Debounced search as the user types.
  useEffect(() => {
    if (!open) return;
    const q = query.trim();
    if (!q) {
      setHits([]);
      setError(null);
      return;
    }
    setLoading(true);
    const reqId = ++queryId.current;
    const handle = setTimeout(() => {
      void searchVault(q, mode, 25)
        .then((rows) => {
          if (reqId !== queryId.current) return;
          setHits(rows);
          setFocusedIdx(0);
          setError(null);
        })
        .catch((e) => {
          if (reqId !== queryId.current) return;
          setError(messageOf(e));
          setHits([]);
        })
        .finally(() => {
          if (reqId === queryId.current) setLoading(false);
        });
    }, 120);
    return () => clearTimeout(handle);
  }, [query, mode, open]);

  const openHit = useCallback(
    (hit: SearchHit) => {
      void useEditorStore.getState().openFile(hit.file_path);
      onClose();
    },
    [onClose]
  );

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center bg-black/40 pt-24">
      <div
        className="w-full max-w-2xl rounded-lg border border-[var(--color-border)] bg-[var(--color-surface)] shadow-2xl overflow-hidden"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center gap-2 px-4 py-3 border-b border-[var(--color-border)]">
          <Search size={14} className="text-[var(--color-text-faint)]" />
          <input
            ref={inputRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape") {
                onClose();
              } else if (e.key === "ArrowDown") {
                e.preventDefault();
                setFocusedIdx((i) => Math.min(i + 1, hits.length - 1));
              } else if (e.key === "ArrowUp") {
                e.preventDefault();
                setFocusedIdx((i) => Math.max(0, i - 1));
              } else if (e.key === "Enter") {
                e.preventDefault();
                const hit = hits[focusedIdx];
                if (hit) openHit(hit);
              }
            }}
            placeholder="Search by meaning or keyword…"
            className="flex-1 bg-transparent outline-none text-[var(--color-text)] placeholder:text-[var(--color-text-faint)]"
          />
          <ModeTabs mode={mode} onChange={setMode} />
          <button
            type="button"
            onClick={onClose}
            className="text-[var(--color-text-faint)] hover:text-[var(--color-text)]"
            title="Close (Esc)"
          >
            <X size={14} />
          </button>
        </div>
        <div className="max-h-[60vh] overflow-auto">
          {loading && (
            <div className="px-4 py-3 text-xs text-[var(--color-text-faint)]">
              Searching…
            </div>
          )}
          {error && (
            <div className="px-4 py-3 text-xs text-red-400">{error}</div>
          )}
          {!loading && !error && query.trim() && hits.length === 0 && (
            <div className="px-4 py-3 text-xs text-[var(--color-text-faint)]">
              No matches.
            </div>
          )}
          {hits.map((hit, idx) => (
            <button
              key={`${hit.block_id}-${idx}`}
              type="button"
              onMouseEnter={() => setFocusedIdx(idx)}
              onClick={() => openHit(hit)}
              className={
                "block w-full text-left px-4 py-2.5 border-l-2 " +
                (idx === focusedIdx
                  ? "bg-[var(--color-surface-hover)] border-[var(--color-accent)]"
                  : "border-transparent hover:bg-[var(--color-surface-hover)]")
              }
            >
              <div className="flex items-center gap-2 text-[12px]">
                <KindIcon blockType={hit.block_type} />
                <span className="text-[var(--color-text)] truncate">
                  {hit.file_title}
                </span>
                <span className="text-[var(--color-text-faint)] truncate">
                  {hit.file_path}
                  {!hit.block_type.startsWith("media:") &&
                    `:${hit.line_number + 1}`}
                </span>
                <span className="ml-auto flex items-center gap-1.5 text-[10px] text-[var(--color-text-faint)]">
                  <span className="px-1.5 py-0.5 rounded bg-[var(--color-bg)] border border-[var(--color-border)] uppercase tracking-wider">
                    {hit.matched_via}
                  </span>
                  <span>{hit.score.toFixed(3)}</span>
                </span>
              </div>
              <div className="text-[11px] text-[var(--color-text-dim)] mt-0.5 line-clamp-2">
                {hit.snippet}
              </div>
            </button>
          ))}
        </div>
        <div className="px-4 py-2 border-t border-[var(--color-border)] text-[10px] text-[var(--color-text-faint)] flex gap-4">
          <span>↑↓ navigate</span>
          <span>↵ open</span>
          <span>esc close</span>
        </div>
      </div>
    </div>
  );
}

function KindIcon({ blockType }: { blockType: string }) {
  if (blockType === "media:audio") {
    return <Music size={11} className="text-[var(--color-accent)] shrink-0" />;
  }
  if (blockType === "media:video") {
    return <FileVideo size={11} className="text-[var(--color-accent)] shrink-0" />;
  }
  if (blockType === "media:image") {
    return <FileImage size={11} className="text-[var(--color-accent)] shrink-0" />;
  }
  return null;
}

function ModeTabs({
  mode,
  onChange,
}: {
  mode: SearchMode;
  onChange: (m: SearchMode) => void;
}) {
  return (
    <div className="flex items-center gap-0.5 rounded border border-[var(--color-border)] p-0.5 text-[11px]">
      {(["hybrid", "semantic", "fts"] as SearchMode[]).map((m) => (
        <button
          key={m}
          type="button"
          onClick={() => onChange(m)}
          className={
            "px-2 py-0.5 rounded " +
            (mode === m
              ? "bg-[var(--color-surface-hover)] text-[var(--color-accent)]"
              : "text-[var(--color-text-faint)] hover:text-[var(--color-text)]")
          }
        >
          {m}
        </button>
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
