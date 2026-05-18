"use client";

import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  Brain,
  Eye,
  Film,
  FolderOpen,
  Network,
  PenLine,
  RefreshCw,
  Save,
  SplitSquareHorizontal,
} from "lucide-react";
import { scanMedia } from "@/lib/tauri/media";
import { AIChat } from "@/components/ai/AIChat";
import { CodeMirrorEditor } from "@/components/editor/CodeMirrorEditor";
import { ReadingView } from "@/components/editor/ReadingView";
import { GraphView } from "@/components/graph/GraphView";
import { SearchPalette } from "@/components/search/SearchPalette";
import { FileExplorer } from "@/components/sidebar/FileExplorer";
import { Backlinks } from "@/components/sidebar/Backlinks";
import { Outline } from "@/components/sidebar/Outline";
import { RelatedNotes } from "@/components/sidebar/RelatedNotes";
import { useVaultStore } from "@/lib/store/vaultStore";
import { useEditorStore } from "@/lib/store/editorStore";
import { reindexVault } from "@/lib/tauri/vault";
import type { VaultChangeEvent } from "@/types/vault";

type ViewMode = "source" | "live" | "reading" | "graph" | "ai";

export default function HomePage() {
  const { info, tree, loading, error, pickAndOpen, refreshTree } =
    useVaultStore();
  const { activePath, content, dirty, saving, save } = useEditorStore();
  const [refreshKey, setRefreshKey] = useState(0);
  const [mode, setMode] = useState<ViewMode>("live");
  const [searchOpen, setSearchOpen] = useState(false);
  const [scanningMedia, setScanningMedia] = useState(false);
  const [mediaToast, setMediaToast] = useState<string | null>(null);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const mod = e.metaKey || e.ctrlKey;
      if (mod && e.shiftKey && (e.key === "F" || e.key === "f")) {
        e.preventDefault();
        setSearchOpen(true);
      } else if (e.key === "Escape" && searchOpen) {
        setSearchOpen(false);
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [searchOpen]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    listen<VaultChangeEvent>("vault://changed", (event) => {
      void useVaultStore.getState().refreshTree();
      setRefreshKey((k) => k + 1);
      const editor = useEditorStore.getState();
      if (
        event.payload.kind === "removed" &&
        editor.activePath === event.payload.path
      ) {
        editor.close();
      }
    }).then((u) => {
      unlisten = u;
    });
    return () => {
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    if (!saving && !dirty) setRefreshKey((k) => k + 1);
  }, [saving, dirty]);

  if (!info) {
    return (
      <main className="h-screen w-screen flex items-center justify-center">
        <div className="text-center max-w-md px-8">
          <div className="text-3xl font-light tracking-tight mb-2">
            <span className="text-[var(--color-accent)]">Aura</span>
          </div>
          <p className="text-[var(--color-text-dim)] mb-8">
            Local-first, AI-native knowledge engine.
          </p>
          <button
            onClick={() => void pickAndOpen()}
            disabled={loading}
            className="inline-flex items-center gap-2 px-5 py-2.5 rounded-md bg-[var(--color-accent)] text-[var(--color-bg)] hover:bg-[var(--color-accent-hover)] transition disabled:opacity-50"
          >
            <FolderOpen size={16} />
            {loading ? "Opening…" : "Open Vault"}
          </button>
          {error && (
            <p className="text-red-400 text-xs mt-6 break-words">{error}</p>
          )}
          <p className="text-[var(--color-text-faint)] text-xs mt-10">
            Pick any folder containing <code>.md</code> files. Aura never moves
            your files; it indexes them in place.
          </p>
        </div>
      </main>
    );
  }

  return (
    <div className="h-screen w-screen flex flex-col">
      <TopBar />
      <div className="flex-1 flex min-h-0">
        <aside className="w-64 shrink-0 border-r border-[var(--color-border)] bg-[var(--color-surface)] flex flex-col">
          <div className="px-3 py-2 text-[11px] uppercase tracking-wider text-[var(--color-text-faint)] flex items-center justify-between">
            <span>Files</span>
            <button
              type="button"
              onClick={() => {
                void reindexVault().then(() => refreshTree());
              }}
              className="hover:text-[var(--color-text)]"
              title="Reindex vault"
            >
              <RefreshCw size={11} />
            </button>
          </div>
          <div className="flex-1 overflow-auto pb-4">
            {tree && <FileExplorer tree={tree} />}
          </div>
          <div className="border-t border-[var(--color-border)] p-2 space-y-1">
            <button
              type="button"
              onClick={async () => {
                setScanningMedia(true);
                setMediaToast(null);
                try {
                  const r = await scanMedia();
                  setMediaToast(
                    `Indexed ${r.ingested} media file${r.ingested === 1 ? "" : "s"}` +
                      (r.skipped > 0 ? ` (${r.skipped} skipped)` : "")
                  );
                } catch (e) {
                  setMediaToast(
                    e && typeof e === "object" && "message" in e
                      ? String((e as { message: unknown }).message)
                      : String(e)
                  );
                } finally {
                  setScanningMedia(false);
                }
              }}
              disabled={scanningMedia}
              className="w-full inline-flex items-center gap-2 px-3 py-1.5 rounded text-[12px] text-[var(--color-text-dim)] hover:bg-[var(--color-surface-hover)] disabled:opacity-50"
              title="Scan vault for audio / video / image files"
            >
              <Film size={13} />
              {scanningMedia ? "Scanning…" : "Scan media"}
            </button>
            {mediaToast && (
              <p className="px-3 py-1 text-[10px] text-[var(--color-text-faint)]">
                {mediaToast}
              </p>
            )}
            <button
              type="button"
              onClick={() => setMode((m) => (m === "graph" ? "live" : "graph"))}
              className={
                "w-full inline-flex items-center gap-2 px-3 py-1.5 rounded text-[12px] " +
                (mode === "graph"
                  ? "bg-[var(--color-surface-hover)] text-[var(--color-accent)]"
                  : "text-[var(--color-text-dim)] hover:bg-[var(--color-surface-hover)]")
              }
              title="Toggle graph view"
            >
              <Network size={13} />
              Graph view
            </button>
            <button
              type="button"
              onClick={() => setMode((m) => (m === "ai" ? "live" : "ai"))}
              className={
                "w-full inline-flex items-center gap-2 px-3 py-1.5 rounded text-[12px] " +
                (mode === "ai"
                  ? "bg-[var(--color-surface-hover)] text-[var(--color-accent)]"
                  : "text-[var(--color-text-dim)] hover:bg-[var(--color-surface-hover)]")
              }
              title="Global GraphRAG query"
            >
              <Brain size={13} />
              Global query
            </button>
          </div>
        </aside>
        <main className="flex-1 min-w-0 flex flex-col bg-[var(--color-bg)]">
          {mode === "graph" ? (
            <GraphView />
          ) : mode === "ai" ? (
            <AIChat />
          ) : activePath ? (
            <>
              <div className="px-4 py-2 border-b border-[var(--color-border)] flex items-center gap-3 text-xs text-[var(--color-text-dim)]">
                <span className="truncate">{activePath}</span>
                {dirty && <span className="text-amber-400">●</span>}
                <ModeToggle mode={mode} onChange={setMode} />
                <button
                  type="button"
                  onClick={() => void save()}
                  disabled={!dirty || saving}
                  className="inline-flex items-center gap-1 px-2 py-1 rounded hover:bg-[var(--color-surface-hover)] disabled:opacity-40"
                  title="Save (Ctrl/Cmd+S)"
                >
                  <Save size={12} />
                  {saving ? "Saving…" : "Save"}
                </button>
              </div>
              <div className="flex-1 min-h-0">
                {mode === "reading" ? (
                  <ReadingView path={activePath} content={content} />
                ) : (
                  <CodeMirrorEditor
                    key={`${activePath}::${mode}`}
                    path={activePath}
                    initialContent={content}
                    liveTransclusion={mode === "live"}
                  />
                )}
              </div>
            </>
          ) : (
            <div className="flex-1 flex items-center justify-center text-[var(--color-text-faint)] text-sm">
              Select a note from the sidebar to start editing.
            </div>
          )}
        </main>
        <aside className="w-72 shrink-0 border-l border-[var(--color-border)] bg-[var(--color-surface)] flex flex-col overflow-auto">
          {activePath ? (
            <>
              <Outline path={activePath} refreshKey={refreshKey} />
              <div className="border-t border-[var(--color-border)] mt-2">
                <Backlinks path={activePath} refreshKey={refreshKey} />
              </div>
              <div className="border-t border-[var(--color-border)] mt-2">
                <RelatedNotes path={activePath} refreshKey={refreshKey} />
              </div>
            </>
          ) : (
            <p className="px-3 py-3 text-[var(--color-text-faint)] text-[11px]">
              Open a note to see its outline and backlinks.
            </p>
          )}
        </aside>
      </div>
      <StatusBar mode={mode} onSearchClick={() => setSearchOpen(true)} />
      <SearchPalette open={searchOpen} onClose={() => setSearchOpen(false)} />
    </div>
  );
}

function ModeToggle({
  mode,
  onChange,
}: {
  mode: ViewMode;
  onChange: (m: ViewMode) => void;
}) {
  return (
    <div className="ml-auto flex items-center gap-0.5 rounded border border-[var(--color-border)] p-0.5">
      <ModeButton
        active={mode === "source"}
        onClick={() => onChange("source")}
        title="Source"
      >
        <PenLine size={12} />
      </ModeButton>
      <ModeButton
        active={mode === "live"}
        onClick={() => onChange("live")}
        title="Live Preview"
      >
        <SplitSquareHorizontal size={12} />
      </ModeButton>
      <ModeButton
        active={mode === "reading"}
        onClick={() => onChange("reading")}
        title="Reading"
      >
        <Eye size={12} />
      </ModeButton>
    </div>
  );
}

function ModeButton({
  active,
  onClick,
  title,
  children,
}: {
  active: boolean;
  onClick: () => void;
  title: string;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      title={title}
      className={
        "inline-flex items-center px-2 py-1 rounded text-[11px] " +
        (active
          ? "bg-[var(--color-surface-hover)] text-[var(--color-accent)]"
          : "text-[var(--color-text-faint)] hover:text-[var(--color-text)]")
      }
    >
      {children}
    </button>
  );
}

function TopBar() {
  const info = useVaultStore((s) => s.info);
  return (
    <header className="h-9 shrink-0 border-b border-[var(--color-border)] flex items-center px-3 text-xs text-[var(--color-text-dim)] gap-3">
      <span className="text-[var(--color-accent)] font-medium">Aura</span>
      <span className="text-[var(--color-text-faint)] truncate">
        {info?.root}
      </span>
    </header>
  );
}

function StatusBar({
  mode,
  onSearchClick,
}: {
  mode: ViewMode;
  onSearchClick: () => void;
}) {
  const info = useVaultStore((s) => s.info);
  const activePath = useEditorStore((s) => s.activePath);
  const modeLabel =
    mode === "source"
      ? "Source"
      : mode === "live"
        ? "Live Preview"
        : mode === "reading"
          ? "Reading"
          : mode === "graph"
            ? "Graph"
            : "Global Query";
  return (
    <footer className="h-6 shrink-0 border-t border-[var(--color-border)] flex items-center px-3 text-[11px] text-[var(--color-text-faint)] gap-4">
      <span>{info?.file_count ?? 0} notes indexed</span>
      {activePath && <span className="truncate">{activePath}</span>}
      <button
        type="button"
        onClick={onSearchClick}
        className="ml-auto inline-flex items-center gap-1 hover:text-[var(--color-text)]"
        title="Search (Ctrl/Cmd+Shift+F)"
      >
        Search · ⇧⌘F
      </button>
      <span>{modeLabel}</span>
    </footer>
  );
}
