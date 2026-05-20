"use client";

import { useEffect, useMemo, useRef, useState } from "react";
import { EmbedCache, expandEmbeds } from "@/lib/embeds";
import { renderMarkdown } from "@/lib/markdown";
import { useEditorStore } from "@/lib/store/editorStore";
import { useVaultStore } from "@/lib/store/vaultStore";

interface Props {
  path: string;
  content: string;
}

export function ReadingView({ path, content }: Props) {
  const cache = useMemo(() => new EmbedCache(), [path]);
  const [html, setHtml] = useState<string>("");
  const hostRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    let cancelled = false;
    setHtml("");
    void expandEmbeds(content, cache, 2)
      .then((expanded) => {
        if (!cancelled) setHtml(renderMarkdown(expanded));
      })
      .catch(() => {
        if (!cancelled) setHtml(renderMarkdown(content));
      });
    return () => {
      cancelled = true;
    };
  }, [path, content, cache]);

  // Delegate clicks on wiki-link anchors to the editor store.
  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    const handler = (e: MouseEvent) => {
      const target = e.target as HTMLElement | null;
      const anchor = target?.closest?.("a.wiki-link") as HTMLElement | null;
      if (!anchor) return;
      e.preventDefault();
      const wikiTarget = anchor.getAttribute("data-wiki-target");
      if (!wikiTarget) return;
      const resolved = useVaultStore.getState().resolveWikiTarget(wikiTarget);
      if (resolved) void useEditorStore.getState().openFile(resolved);
    };
    host.addEventListener("click", handler);
    return () => host.removeEventListener("click", handler);
  }, [html]);

  return (
    <div
      ref={hostRef}
      className="aura-prose h-full w-full overflow-auto px-7 py-6"
      // The rendered output is generated from the user's own local files,
      // which Aura treats as trusted. There is no remote content path here.
      dangerouslySetInnerHTML={{ __html: html }}
    />
  );
}
