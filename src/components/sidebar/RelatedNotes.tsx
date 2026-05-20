"use client";

import { useEffect, useState } from "react";
import { Sparkles } from "lucide-react";
import { findRelated } from "@/lib/tauri/related";
import type { RelatedNote } from "@/types/vault";
import { useEditorStore } from "@/lib/store/editorStore";

interface Props {
  path: string;
  refreshKey?: number;
}

export function RelatedNotes({ path, refreshKey = 0 }: Props) {
  const [items, setItems] = useState<RelatedNote[]>([]);
  const [loading, setLoading] = useState(false);
  const openFile = useEditorStore((s) => s.openFile);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    findRelated(path, 10)
      .then((rows) => {
        if (!cancelled) setItems(rows);
      })
      .catch(() => {
        if (!cancelled) setItems([]);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [path, refreshKey]);

  return (
    <div className="text-[12px]">
      <div className="px-3 py-2 flex items-center gap-2 text-[11px] uppercase tracking-wider text-[var(--color-text-faint)]">
        <Sparkles size={11} />
        <span>Related (HDC)</span>
        <span className="ml-auto text-[var(--color-text-faint)]">
          {loading ? "…" : items.length}
        </span>
      </div>
      {items.length === 0 && !loading && (
        <p className="px-3 py-2 text-[var(--color-text-faint)] text-[11px]">
          No related notes yet.
        </p>
      )}
      <ul>
        {items.map((r) => (
          <li key={r.file_id}>
            <button
              type="button"
              onClick={() => void openFile(r.path)}
              className="block w-full text-left px-3 py-2 hover:bg-[var(--color-surface-hover)]"
            >
              <div className="flex items-center gap-2">
                <span className="text-[var(--color-text)] truncate flex-1">
                  {r.title}
                </span>
                <span className="text-[10px] text-[var(--color-text-faint)] tabular-nums">
                  {r.score.toFixed(3)}
                </span>
              </div>
              <div className="text-[var(--color-text-faint)] text-[10px] truncate flex gap-2">
                <span className="truncate flex-1">{r.path}</span>
                {r.shared_neighbours > 0 && (
                  <span title="Shared neighbours">
                    ⇄ {r.shared_neighbours}
                  </span>
                )}
              </div>
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}
