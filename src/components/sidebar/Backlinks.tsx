"use client";

import { useEffect, useState } from "react";
import { Link2 } from "lucide-react";
import { getBacklinks } from "@/lib/tauri/link";
import type { BacklinkEntry } from "@/types/vault";
import { useEditorStore } from "@/lib/store/editorStore";

interface Props {
  path: string;
  refreshKey?: number;
}

export function Backlinks({ path, refreshKey = 0 }: Props) {
  const [items, setItems] = useState<BacklinkEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const openFile = useEditorStore((s) => s.openFile);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    getBacklinks(path)
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
        <Link2 size={11} />
        <span>Backlinks</span>
        <span className="ml-auto text-[var(--color-text-faint)]">
          {loading ? "…" : items.length}
        </span>
      </div>
      {items.length === 0 && !loading && (
        <p className="px-3 py-2 text-[var(--color-text-faint)] text-[11px]">
          No notes link here yet.
        </p>
      )}
      <ul>
        {items.map((b, i) => (
          <li key={`${b.source_file_id}-${b.line_number}-${i}`}>
            <button
              type="button"
              onClick={() => void openFile(b.source_path)}
              className="block w-full text-left px-3 py-2 hover:bg-[var(--color-surface-hover)]"
            >
              <div className="text-[var(--color-text)] truncate">
                {b.source_title}
              </div>
              <div className="text-[var(--color-text-faint)] text-[10px] truncate">
                {b.source_path}:{b.line_number + 1}
              </div>
              {b.context && (
                <div className="text-[var(--color-text-dim)] mt-1 line-clamp-2">
                  {b.context}
                </div>
              )}
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}
