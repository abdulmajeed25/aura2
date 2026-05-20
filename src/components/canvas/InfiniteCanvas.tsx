"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import {
  FilePlus2,
  Layers,
  Plus,
  RefreshCw,
  Save,
  StickyNote,
} from "lucide-react";
import {
  createCanvas,
  listCanvases,
  readCanvas,
  writeCanvas,
} from "@/lib/tauri/canvas";
import { useEditorStore } from "@/lib/store/editorStore";
import { useVaultStore } from "@/lib/store/vaultStore";
import type { CanvasDoc, CanvasFile, CanvasNode } from "@/types/vault";

const CARD_RADIUS = 8;
const HEADER_HEIGHT = 26;
const DEFAULT_CARD_W = 280;
const DEFAULT_CARD_H = 160;

interface Viewport {
  x: number;
  y: number;
  scale: number;
}

type DragState =
  | { kind: "pan"; startX: number; startY: number; viewport: Viewport }
  | { kind: "card"; id: string; offsetX: number; offsetY: number };

export function InfiniteCanvas() {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const viewportRef = useRef<Viewport>({ x: 60, y: 60, scale: 1 });
  const docRef = useRef<CanvasDoc>({ nodes: [], edges: [] });
  const dragRef = useRef<DragState | null>(null);
  const hoverIdRef = useRef<string | null>(null);
  const dirtyRef = useRef(false);

  const [available, setAvailable] = useState<CanvasFile[]>([]);
  const [active, setActive] = useState<string | null>(null);
  const [doc, setDoc] = useState<CanvasDoc>({ nodes: [], edges: [] });
  const [dirty, setDirty] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const candidates = useVaultStore((s) => s.candidates);
  const openFile = useEditorStore((s) => s.openFile);

  const titleFor = useCallback(
    (path: string) =>
      candidates.find((c) => c.path === path)?.title ??
      path.replace(/\.(md|markdown)$/i, ""),
    [candidates]
  );

  // Keep docRef in sync with state so the imperative draw loop has fresh data.
  useEffect(() => {
    docRef.current = doc;
    dirtyRef.current = dirty;
  }, [doc, dirty]);

  const refreshList = useCallback(async () => {
    try {
      const list = await listCanvases();
      setAvailable(list);
      if (!active && list.length > 0) setActive(list[0].path);
    } catch (e) {
      setError(messageOf(e));
    }
  }, [active]);

  useEffect(() => {
    void refreshList();
  }, [refreshList]);

  useEffect(() => {
    if (!active) return;
    let cancelled = false;
    void readCanvas(active)
      .then((d) => {
        if (!cancelled) {
          setDoc(d);
          setDirty(false);
        }
      })
      .catch((e) => {
        if (!cancelled) setError(messageOf(e));
      });
    return () => {
      cancelled = true;
    };
  }, [active]);

  const draw = useCallback(() => {
    const canvas = canvasRef.current;
    const container = containerRef.current;
    if (!canvas || !container) return;

    const dpr = window.devicePixelRatio || 1;
    const rect = container.getBoundingClientRect();
    if (canvas.width !== rect.width * dpr || canvas.height !== rect.height * dpr) {
      canvas.width = rect.width * dpr;
      canvas.height = rect.height * dpr;
      canvas.style.width = `${rect.width}px`;
      canvas.style.height = `${rect.height}px`;
    }
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.fillStyle = "#0a0a0f";
    ctx.fillRect(0, 0, rect.width, rect.height);

    const vp = viewportRef.current;
    const d = docRef.current;
    const lookup = new Map(d.nodes.map((n) => [n.id, n]));

    // Edges first (under cards).
    ctx.strokeStyle = "rgba(140, 140, 160, 0.6)";
    ctx.lineWidth = 1.5;
    for (const e of d.edges) {
      const a = lookup.get(e.fromNode);
      const b = lookup.get(e.toNode);
      if (!a || !b) continue;
      const ax = (a.x + a.width / 2) * vp.scale + vp.x;
      const ay = (a.y + a.height / 2) * vp.scale + vp.y;
      const bx = (b.x + b.width / 2) * vp.scale + vp.x;
      const by = (b.y + b.height / 2) * vp.scale + vp.y;
      ctx.beginPath();
      ctx.moveTo(ax, ay);
      ctx.lineTo(bx, by);
      ctx.stroke();
      if (e.label) {
        ctx.fillStyle = "#a1a1aa";
        ctx.font = "10px ui-sans-serif, system-ui";
        ctx.textAlign = "center";
        ctx.fillText(e.label, (ax + bx) / 2, (ay + by) / 2 - 4);
      }
    }

    // Cards.
    for (const n of d.nodes) {
      const x = n.x * vp.scale + vp.x;
      const y = n.y * vp.scale + vp.y;
      const w = n.width * vp.scale;
      const h = n.height * vp.scale;
      const isHover = hoverIdRef.current === n.id;

      ctx.fillStyle = "#14141b";
      ctx.strokeStyle = isHover ? "#818cf8" : "#26262f";
      ctx.lineWidth = isHover ? 2 : 1;
      roundRect(ctx, x, y, w, h, CARD_RADIUS);
      ctx.fill();
      ctx.stroke();

      // Header.
      ctx.fillStyle = n.type === "file" ? "#1c2233" : "#221c33";
      ctx.beginPath();
      ctx.moveTo(x, y + CARD_RADIUS);
      ctx.arcTo(x, y, x + CARD_RADIUS, y, CARD_RADIUS);
      ctx.lineTo(x + w - CARD_RADIUS, y);
      ctx.arcTo(x + w, y, x + w, y + CARD_RADIUS, CARD_RADIUS);
      ctx.lineTo(x + w, y + HEADER_HEIGHT);
      ctx.lineTo(x, y + HEADER_HEIGHT);
      ctx.closePath();
      ctx.fill();

      // Title.
      ctx.fillStyle = "#e4e4e7";
      ctx.font = "600 12px ui-sans-serif, system-ui";
      ctx.textAlign = "left";
      ctx.textBaseline = "middle";
      const title =
        n.type === "file" ? `↳ ${titleFor(n.file)}` : "✦ Note";
      ctx.fillText(clip(title, w - 16), x + 10, y + HEADER_HEIGHT / 2);

      // Body.
      ctx.fillStyle = "#a1a1aa";
      ctx.font = "11px ui-sans-serif, system-ui";
      ctx.textBaseline = "top";
      const body = n.type === "file" ? n.file : n.text;
      wrapText(
        ctx,
        body,
        x + 10,
        y + HEADER_HEIGHT + 8,
        w - 20,
        14,
        Math.floor((h - HEADER_HEIGHT - 16) / 14)
      );
    }
  }, [titleFor]);

  // Redraw whenever doc / candidates change.
  useEffect(() => {
    requestAnimationFrame(draw);
  }, [doc, candidates, draw]);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    const ro = new ResizeObserver(() => requestAnimationFrame(draw));
    ro.observe(container);
    return () => ro.disconnect();
  }, [draw]);

  const hitTest = useCallback((px: number, py: number): CanvasNode | null => {
    const vp = viewportRef.current;
    const d = docRef.current;
    // Iterate in reverse so the topmost (last drawn) card wins.
    for (let i = d.nodes.length - 1; i >= 0; i--) {
      const n = d.nodes[i];
      const x = n.x * vp.scale + vp.x;
      const y = n.y * vp.scale + vp.y;
      const w = n.width * vp.scale;
      const h = n.height * vp.scale;
      if (px >= x && px <= x + w && py >= y && py <= y + h) {
        return n;
      }
    }
    return null;
  }, []);

  // Mouse handling.
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    const onDown = (e: MouseEvent) => {
      const rect = canvas.getBoundingClientRect();
      const px = e.clientX - rect.left;
      const py = e.clientY - rect.top;
      const hit = hitTest(px, py);
      if (hit) {
        const vp = viewportRef.current;
        const cardX = hit.x * vp.scale + vp.x;
        const cardY = hit.y * vp.scale + vp.y;
        dragRef.current = {
          kind: "card",
          id: hit.id,
          offsetX: px - cardX,
          offsetY: py - cardY,
        };
      } else {
        dragRef.current = {
          kind: "pan",
          startX: px,
          startY: py,
          viewport: { ...viewportRef.current },
        };
      }
    };

    const onMove = (e: MouseEvent) => {
      const rect = canvas.getBoundingClientRect();
      const px = e.clientX - rect.left;
      const py = e.clientY - rect.top;
      const drag = dragRef.current;
      if (drag?.kind === "pan") {
        viewportRef.current = {
          ...drag.viewport,
          x: drag.viewport.x + (px - drag.startX),
          y: drag.viewport.y + (py - drag.startY),
        };
        requestAnimationFrame(draw);
        return;
      }
      if (drag?.kind === "card") {
        const vp = viewportRef.current;
        const newCardX = (px - drag.offsetX - vp.x) / vp.scale;
        const newCardY = (py - drag.offsetY - vp.y) / vp.scale;
        const next: CanvasDoc = {
          ...docRef.current,
          nodes: docRef.current.nodes.map((n) =>
            n.id === drag.id ? { ...n, x: newCardX, y: newCardY } : n
          ),
        };
        docRef.current = next;
        dirtyRef.current = true;
        requestAnimationFrame(draw);
        return;
      }
      const hit = hitTest(px, py);
      const newId = hit?.id ?? null;
      if (newId !== hoverIdRef.current) {
        hoverIdRef.current = newId;
        canvas.style.cursor = newId ? "pointer" : "default";
        requestAnimationFrame(draw);
      }
    };

    const onUp = (e: MouseEvent) => {
      const drag = dragRef.current;
      dragRef.current = null;
      if (drag?.kind === "card") {
        // Commit the dragged position to React state so persistence + dirty flag follow.
        setDoc(docRef.current);
        setDirty(true);
      } else if (drag?.kind === "pan") {
        // Pan release: treat tiny movements as a click on a card if any.
        const rect = canvas.getBoundingClientRect();
        const px = e.clientX - rect.left;
        const py = e.clientY - rect.top;
        if (
          Math.abs(px - drag.startX) < 4 &&
          Math.abs(py - drag.startY) < 4
        ) {
          const hit = hitTest(px, py);
          if (hit && hit.type === "file") {
            void openFile(hit.file);
          }
        }
      }
    };

    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const rect = canvas.getBoundingClientRect();
      const px = e.clientX - rect.left;
      const py = e.clientY - rect.top;
      const vp = viewportRef.current;
      const factor = Math.exp(-e.deltaY * 0.001);
      const newScale = Math.min(Math.max(vp.scale * factor, 0.2), 4);
      vp.x = px - ((px - vp.x) * newScale) / vp.scale;
      vp.y = py - ((py - vp.y) * newScale) / vp.scale;
      vp.scale = newScale;
      requestAnimationFrame(draw);
    };

    canvas.addEventListener("mousedown", onDown);
    canvas.addEventListener("mousemove", onMove);
    canvas.addEventListener("mouseup", onUp);
    canvas.addEventListener("mouseleave", () => {
      dragRef.current = null;
      hoverIdRef.current = null;
      requestAnimationFrame(draw);
    });
    canvas.addEventListener("wheel", onWheel, { passive: false });
    return () => {
      canvas.removeEventListener("mousedown", onDown);
      canvas.removeEventListener("mousemove", onMove);
      canvas.removeEventListener("mouseup", onUp);
      canvas.removeEventListener("wheel", onWheel);
    };
  }, [draw, hitTest, openFile]);

  const addFileCard = () => {
    const path = window.prompt("Vault-relative path to the note (e.g. Welcome.md):");
    if (!path) return;
    const vp = viewportRef.current;
    const baseX = (200 - vp.x) / vp.scale;
    const baseY = (160 - vp.y) / vp.scale;
    const next: CanvasDoc = {
      ...doc,
      nodes: [
        ...doc.nodes,
        {
          type: "file",
          id: crypto.randomUUID(),
          x: baseX,
          y: baseY,
          width: DEFAULT_CARD_W,
          height: DEFAULT_CARD_H,
          file: path,
        },
      ],
    };
    setDoc(next);
    setDirty(true);
  };

  const addTextCard = () => {
    const text = window.prompt("Card text:") ?? "";
    const vp = viewportRef.current;
    const baseX = (240 - vp.x) / vp.scale;
    const baseY = (200 - vp.y) / vp.scale;
    const next: CanvasDoc = {
      ...doc,
      nodes: [
        ...doc.nodes,
        {
          type: "text",
          id: crypto.randomUUID(),
          x: baseX,
          y: baseY,
          width: DEFAULT_CARD_W,
          height: DEFAULT_CARD_H,
          text,
        },
      ],
    };
    setDoc(next);
    setDirty(true);
  };

  const save = async () => {
    if (!active) return;
    setSaving(true);
    setError(null);
    try {
      await writeCanvas(active, doc);
      setDirty(false);
    } catch (e) {
      setError(messageOf(e));
    } finally {
      setSaving(false);
    }
  };

  const create = async () => {
    const path = window.prompt("New canvas path (must end in .canvas):");
    if (!path || !path.endsWith(".canvas")) {
      setError("Canvas path must end with .canvas");
      return;
    }
    try {
      await createCanvas(path);
      await refreshList();
      setActive(path);
    } catch (e) {
      setError(messageOf(e));
    }
  };

  return (
    <div className="h-full w-full flex flex-col bg-[var(--color-bg)]">
      <div className="px-3 py-2 border-b border-[var(--color-border)] flex items-center gap-2 text-xs text-[var(--color-text-dim)]">
        <Layers size={13} className="text-[var(--color-accent)]" />
        <select
          value={active ?? ""}
          onChange={(e) => setActive(e.target.value || null)}
          className="bg-[var(--color-surface)] border border-[var(--color-border)] rounded px-2 py-1 text-[12px] text-[var(--color-text)]"
        >
          <option value="">— pick a canvas —</option>
          {available.map((c) => (
            <option key={c.path} value={c.path}>
              {c.path}
            </option>
          ))}
        </select>
        <button
          type="button"
          onClick={() => void create()}
          className="inline-flex items-center gap-1 px-2 py-1 rounded border border-[var(--color-border)] hover:bg-[var(--color-surface-hover)]"
          title="New canvas"
        >
          <Plus size={11} />
          New
        </button>
        <button
          type="button"
          onClick={() => void refreshList()}
          className="inline-flex items-center gap-1 text-[var(--color-text-faint)] hover:text-[var(--color-text)]"
          title="Refresh canvas list"
        >
          <RefreshCw size={11} />
        </button>
        <span className="ml-2 text-[var(--color-text-faint)]">
          {doc.nodes.length} cards · {doc.edges.length} edges
        </span>
        {dirty && <span className="text-amber-400">●</span>}
        <button
          type="button"
          onClick={addFileCard}
          disabled={!active}
          className="ml-auto inline-flex items-center gap-1 px-2 py-1 rounded border border-[var(--color-border)] hover:bg-[var(--color-surface-hover)] disabled:opacity-40"
        >
          <FilePlus2 size={11} />
          File card
        </button>
        <button
          type="button"
          onClick={addTextCard}
          disabled={!active}
          className="inline-flex items-center gap-1 px-2 py-1 rounded border border-[var(--color-border)] hover:bg-[var(--color-surface-hover)] disabled:opacity-40"
        >
          <StickyNote size={11} />
          Text card
        </button>
        <button
          type="button"
          onClick={() => void save()}
          disabled={!active || !dirty || saving}
          className="inline-flex items-center gap-1 px-2 py-1 rounded bg-[var(--color-accent)] text-[var(--color-bg)] hover:bg-[var(--color-accent-hover)] disabled:opacity-40"
          title="Save canvas (Ctrl/Cmd+S)"
        >
          <Save size={11} />
          {saving ? "Saving…" : "Save"}
        </button>
      </div>
      <div ref={containerRef} className="flex-1 min-h-0 relative">
        <canvas ref={canvasRef} className="absolute inset-0" />
        {!active && (
          <div className="absolute inset-0 flex items-center justify-center text-[var(--color-text-faint)] text-sm">
            Create or pick a canvas to start.
          </div>
        )}
        {error && (
          <div className="absolute bottom-3 left-3 right-3 rounded border border-red-700 bg-red-950/40 px-3 py-2 text-red-300 text-[12px]">
            {error}
          </div>
        )}
      </div>
    </div>
  );
}

function roundRect(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  w: number,
  h: number,
  r: number
) {
  const rr = Math.min(r, w / 2, h / 2);
  ctx.beginPath();
  ctx.moveTo(x + rr, y);
  ctx.arcTo(x + w, y, x + w, y + h, rr);
  ctx.arcTo(x + w, y + h, x, y + h, rr);
  ctx.arcTo(x, y + h, x, y, rr);
  ctx.arcTo(x, y, x + w, y, rr);
  ctx.closePath();
}

function clip(text: string, maxWidthPx: number): string {
  // crude: assume average 6px per character.
  const maxChars = Math.max(8, Math.floor(maxWidthPx / 6));
  return text.length > maxChars ? text.slice(0, maxChars - 1) + "…" : text;
}

function wrapText(
  ctx: CanvasRenderingContext2D,
  text: string,
  x: number,
  y: number,
  maxWidth: number,
  lineHeight: number,
  maxLines: number
) {
  if (maxLines <= 0) return;
  const words = text.split(/\s+/).filter((w) => w.length > 0);
  let line = "";
  let lines = 0;
  for (const w of words) {
    const tentative = line ? `${line} ${w}` : w;
    if (ctx.measureText(tentative).width > maxWidth && line) {
      ctx.fillText(line, x, y + lines * lineHeight);
      lines += 1;
      if (lines >= maxLines) {
        ctx.fillText("…", x, y + (lines - 1) * lineHeight);
        return;
      }
      line = w;
    } else {
      line = tentative;
    }
  }
  if (line) {
    ctx.fillText(line, x, y + lines * lineHeight);
  }
}

function messageOf(e: unknown): string {
  if (e && typeof e === "object" && "message" in e) {
    return String((e as { message: unknown }).message);
  }
  return String(e);
}
