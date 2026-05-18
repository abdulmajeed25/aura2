"use client";

import { useEffect, useState } from "react";
import { Hash } from "lucide-react";
import { getOutline } from "@/lib/tauri/link";
import type { HeadingEntry } from "@/types/vault";

interface Props {
  path: string;
  refreshKey?: number;
}

export function Outline({ path, refreshKey = 0 }: Props) {
  const [items, setItems] = useState<HeadingEntry[]>([]);

  useEffect(() => {
    let cancelled = false;
    getOutline(path)
      .then((rows) => {
        if (!cancelled) setItems(rows);
      })
      .catch(() => {
        if (!cancelled) setItems([]);
      });
    return () => {
      cancelled = true;
    };
  }, [path, refreshKey]);

  return (
    <div className="text-[12px]">
      <div className="px-3 py-2 flex items-center gap-2 text-[11px] uppercase tracking-wider text-[var(--color-text-faint)]">
        <Hash size={11} />
        <span>Outline</span>
        <span className="ml-auto text-[var(--color-text-faint)]">
          {items.length}
        </span>
      </div>
      {items.length === 0 && (
        <p className="px-3 py-2 text-[var(--color-text-faint)] text-[11px]">
          No headings.
        </p>
      )}
      <ul>
        {items.map((h, i) => (
          <li key={`${i}-${h.line_number}`}>
            <div
              className="px-3 py-1 text-[var(--color-text-dim)] truncate"
              style={{ paddingLeft: 12 + (h.level - 1) * 12 }}
              title={h.text}
            >
              {h.text}
            </div>
          </li>
        ))}
      </ul>
    </div>
  );
}
