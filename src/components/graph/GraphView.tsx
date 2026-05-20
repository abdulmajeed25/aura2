"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { RefreshCw, Search } from "lucide-react";
import { getGraphSnapshot } from "@/lib/tauri/graph";
import { useEditorStore } from "@/lib/store/editorStore";
import { useVaultStore } from "@/lib/store/vaultStore";
import type { GraphNode, GraphSnapshot } from "@/types/vault";

const NODE_BASE_RADIUS = 4;
const NODE_MAX_RADIUS = 12;

interface Viewport {
  x: number;
  y: number;
  scale: number;
}

export function GraphView() {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const snapshotRef = useRef<GraphSnapshot | null>(null);
  const viewportRef = useRef<Viewport>({ x: 0, y: 0, scale: 1 });
  const filterRef = useRef<string>("");
  const hoverIdRef = useRef<string | null>(null);
  const dragRef = useRef<{ x: number; y: number; moved: boolean } | null>(null);

  const [snapshot, setSnapshot] = useState<GraphSnapshot | null>(null);
  const [loading, setLoading] = useState(false);
  const [filter, setFilter] = useState("");
  const [error, setError] = useState<string | null>(null);

  const activePath = useEditorStore((s) => s.activePath);
  const refreshKey = useVaultStore((s) => s.candidates.length);

  const matchesFilter = useCallback((node: GraphNode, q: string) => {
    if (!q) return true;
    const lower = q.toLowerCase();
    return (
      node.title.toLowerCase().includes(lower) ||
      node.path.toLowerCase().includes(lower)
    );
  }, []);

  const fitToView = useCallback((snap: GraphSnapshot) => {
    if (!containerRef.current || snap.nodes.length === 0) return;
    const rect = containerRef.current.getBoundingClientRect();
    if (rect.width === 0 || rect.height === 0) return;

    let minX = Infinity;
    let minY = Infinity;
    let maxX = -Infinity;
    let maxY = -Infinity;
    for (const n of snap.nodes) {
      if (n.x < minX) minX = n.x;
      if (n.y < minY) minY = n.y;
      if (n.x > maxX) maxX = n.x;
      if (n.y > maxY) maxY = n.y;
    }
    const spanX = Math.max(maxX - minX, 1);
    const spanY = Math.max(maxY - minY, 1);
    const margin = 40;
    const scale = Math.min(
      (rect.width - margin * 2) / spanX,
      (rect.height - margin * 2) / spanY,
      4
    );
    const centerX = (minX + maxX) / 2;
    const centerY = (minY + maxY) / 2;
    viewportRef.current = {
      scale,
      x: rect.width / 2 - centerX * scale,
      y: rect.height / 2 - centerY * scale,
    };
  }, []);

  const draw = useCallback(() => {
    const canvas = canvasRef.current;
    const container = containerRef.current;
    const snap = snapshotRef.current;
    if (!canvas || !container || !snap) return;

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
    const q = filterRef.current;

    const lookup = new Map<string, GraphNode>();
    for (const n of snap.nodes) lookup.set(n.id, n);

    const matchSet = new Set<string>();
    if (q) {
      for (const n of snap.nodes) {
        if (matchesFilter(n, q)) matchSet.add(n.id);
      }
    }

    // Edges
    ctx.lineWidth = 1;
    for (const e of snap.edges) {
      const a = lookup.get(e.source);
      const b = lookup.get(e.target);
      if (!a || !b) continue;
      const dim =
        q && !(matchSet.has(a.id) || matchSet.has(b.id)) ? true : false;
      ctx.strokeStyle = dim ? "rgba(60, 60, 70, 0.4)" : "rgba(120, 120, 140, 0.5)";
      ctx.beginPath();
      ctx.moveTo(a.x * vp.scale + vp.x, a.y * vp.scale + vp.y);
      ctx.lineTo(b.x * vp.scale + vp.x, b.y * vp.scale + vp.y);
      ctx.stroke();
    }

    // Nodes
    const hoverId = hoverIdRef.current;
    for (const n of snap.nodes) {
      const r = Math.min(
        NODE_MAX_RADIUS,
        NODE_BASE_RADIUS + Math.sqrt(n.degree) * 1.8
      );
      const cx = n.x * vp.scale + vp.x;
      const cy = n.y * vp.scale + vp.y;
      const isActive = n.path === activePath;
      const isHover = n.id === hoverId;
      const isMatch = !q || matchSet.has(n.id);

      ctx.beginPath();
      ctx.arc(cx, cy, r, 0, Math.PI * 2);
      if (isActive) {
        ctx.fillStyle = "#818cf8";
      } else if (isHover) {
        ctx.fillStyle = "#a5b4fc";
      } else if (!isMatch) {
        ctx.fillStyle = "rgba(140, 140, 160, 0.25)";
      } else {
        ctx.fillStyle = "#6366f1";
      }
      ctx.fill();
      if (isActive || isHover) {
        ctx.strokeStyle = "#e4e4e7";
        ctx.lineWidth = 1.2;
        ctx.stroke();
      }

      // Labels only when zoomed in or hovered.
      if ((vp.scale > 1.4 && isMatch) || isHover || isActive) {
        ctx.fillStyle = isHover || isActive ? "#e4e4e7" : "#a1a1aa";
        ctx.font = "11px ui-sans-serif, system-ui";
        ctx.textAlign = "center";
        ctx.textBaseline = "top";
        const stem = n.path.replace(/\.(md|markdown)$/i, "");
        ctx.fillText(n.title || stem, cx, cy + r + 3);
      }
    }
  }, [activePath, matchesFilter]);

  const hitTest = useCallback(
    (px: number, py: number): GraphNode | null => {
      const snap = snapshotRef.current;
      if (!snap) return null;
      const vp = viewportRef.current;
      let best: GraphNode | null = null;
      let bestDist = Infinity;
      for (const n of snap.nodes) {
        const r = Math.min(
          NODE_MAX_RADIUS,
          NODE_BASE_RADIUS + Math.sqrt(n.degree) * 1.8
        );
        const cx = n.x * vp.scale + vp.x;
        const cy = n.y * vp.scale + vp.y;
        const dx = px - cx;
        const dy = py - cy;
        const d = dx * dx + dy * dy;
        const max = (r + 4) * (r + 4);
        if (d <= max && d < bestDist) {
          best = n;
          bestDist = d;
        }
      }
      return best;
    },
    []
  );

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const snap = await getGraphSnapshot({ iterations: 250 });
      snapshotRef.current = snap;
      setSnapshot(snap);
      fitToView(snap);
      requestAnimationFrame(draw);
    } catch (e) {
      setError(messageOf(e));
    } finally {
      setLoading(false);
    }
  }, [draw, fitToView]);

  useEffect(() => {
    void refresh();
  }, [refresh, refreshKey]);

  useEffect(() => {
    filterRef.current = filter;
    requestAnimationFrame(draw);
  }, [filter, draw]);

  useEffect(() => {
    requestAnimationFrame(draw);
  }, [activePath, draw]);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    const ro = new ResizeObserver(() => requestAnimationFrame(draw));
    ro.observe(container);
    return () => ro.disconnect();
  }, [draw]);

  // Mouse interaction.
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    const onMove = (e: MouseEvent) => {
      const rect = canvas.getBoundingClientRect();
      const px = e.clientX - rect.left;
      const py = e.clientY - rect.top;
      if (dragRef.current) {
        const dx = px - dragRef.current.x;
        const dy = py - dragRef.current.y;
        if (Math.abs(dx) > 2 || Math.abs(dy) > 2) dragRef.current.moved = true;
        viewportRef.current.x += dx;
        viewportRef.current.y += dy;
        dragRef.current.x = px;
        dragRef.current.y = py;
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
    const onDown = (e: MouseEvent) => {
      const rect = canvas.getBoundingClientRect();
      dragRef.current = {
        x: e.clientX - rect.left,
        y: e.clientY - rect.top,
        moved: false,
      };
    };
    const onUp = (e: MouseEvent) => {
      const drag = dragRef.current;
      dragRef.current = null;
      if (drag && !drag.moved) {
        const rect = canvas.getBoundingClientRect();
        const hit = hitTest(e.clientX - rect.left, e.clientY - rect.top);
        if (hit) {
          void useEditorStore.getState().openFile(hit.path);
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
      const newScale = Math.min(Math.max(vp.scale * factor, 0.1), 8);
      // Zoom around cursor.
      vp.x = px - ((px - vp.x) * newScale) / vp.scale;
      vp.y = py - ((py - vp.y) * newScale) / vp.scale;
      vp.scale = newScale;
      requestAnimationFrame(draw);
    };

    canvas.addEventListener("mousemove", onMove);
    canvas.addEventListener("mousedown", onDown);
    canvas.addEventListener("mouseup", onUp);
    canvas.addEventListener("mouseleave", () => {
      dragRef.current = null;
      hoverIdRef.current = null;
      requestAnimationFrame(draw);
    });
    canvas.addEventListener("wheel", onWheel, { passive: false });

    return () => {
      canvas.removeEventListener("mousemove", onMove);
      canvas.removeEventListener("mousedown", onDown);
      canvas.removeEventListener("mouseup", onUp);
      canvas.removeEventListener("wheel", onWheel);
    };
  }, [draw, hitTest]);

  const counts = useMemo(() => {
    if (!snapshot) return { nodes: 0, edges: 0 };
    return { nodes: snapshot.nodes.length, edges: snapshot.edges.length };
  }, [snapshot]);

  return (
    <div className="h-full w-full flex flex-col bg-[var(--color-bg)]">
      <div className="px-3 py-2 border-b border-[var(--color-border)] flex items-center gap-3 text-xs text-[var(--color-text-dim)]">
        <div className="flex items-center gap-1.5 px-2 py-1 rounded border border-[var(--color-border)] bg-[var(--color-surface)]">
          <Search size={11} />
          <input
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
            placeholder="Filter by title or path"
            className="bg-transparent outline-none w-48 text-[var(--color-text)] placeholder:text-[var(--color-text-faint)]"
          />
        </div>
        <span>
          {counts.nodes} notes · {counts.edges} links
        </span>
        {loading && <span className="text-[var(--color-text-faint)]">computing layout…</span>}
        <button
          type="button"
          onClick={() => void refresh()}
          className="ml-auto inline-flex items-center gap-1 px-2 py-1 rounded hover:bg-[var(--color-surface-hover)]"
          title="Recompute layout"
        >
          <RefreshCw size={12} />
          Recompute
        </button>
      </div>
      <div ref={containerRef} className="flex-1 min-h-0 relative">
        <canvas ref={canvasRef} className="absolute inset-0" />
        {error && (
          <div className="absolute inset-0 flex items-center justify-center text-red-400 text-xs">
            {error}
          </div>
        )}
      </div>
    </div>
  );
}

function messageOf(e: unknown): string {
  if (e && typeof e === "object" && "message" in e) {
    return String((e as { message: unknown }).message);
  }
  return String(e);
}
