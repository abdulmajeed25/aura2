"use client";

import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  Eye,
  FolderOpen,
  Network,
  PenLine,
  RefreshCw,
  Save,
  SplitSquareHorizontal,
} from "lucide-react";
import { CodeMirrorEditor } from "@/components/editor/CodeMirrorEditor";
import { ReadingView } from "@/components/editor/ReadingView";
import { GraphView } from "@/components/graph/GraphView";
import { FileExplorer } from "@/components/sidebar/FileExplorer";
import { Backlinks } from "@/components/sidebar/Backlinks";
import { Outline } from "@/components/sidebar/Outline";
import { useVaultStore } from "@/lib/store/vaultStore";
import { useEditorStore } from "@/lib/store/editorStore";
import { reindexVault } from "@/lib/tauri/vault";
import type { VaultChangeEvent } from "@/types/vault";

type ViewMode = "source" | "live" | "reading" | "graph";

export default function HomePage() {
  const { info, tree, loading, error, pickAndOpen, refreshTree } =
    useVaultStore();
  const { activePath, content, dirty, saving, save } = useEditorStore();
  const [refreshKey, setRefreshKey] = useState(0);
  const [mode, setMode] = useState<ViewMode>("live");

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
          <div className="border-t border-[var(--color-border)] p-2">
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
          </div>
        </aside>
        <main className="flex-1 min-w-0 flex flex-col bg-[var(--color-bg)]">
          {mode === "graph" ? (
            <GraphView />
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
            </>
          ) : (
            <p className="px-3 py-3 text-[var(--color-text-faint)] text-[11px]">
              Open a note to see its outline and backlinks.
            </p>
          )}
        </aside>
      </div>
      <StatusBar mode={mode} />
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

function StatusBar({ mode }: { mode: ViewMode }) {
  const info = useVaultStore((s) => s.info);
  const activePath = useEditorStore((s) => s.activePath);
  const modeLabel =
    mode === "source"
      ? "Source"
      : mode === "live"
        ? "Live Preview"
        : mode === "reading"
          ? "Reading"
          : "Graph";
  return (
    <footer className="h-6 shrink-0 border-t border-[var(--color-border)] flex items-center px-3 text-[11px] text-[var(--color-text-faint)] gap-4">
      <span>{info?.file_count ?? 0} notes indexed</span>
      {activePath && <span className="truncate">{activePath}</span>}
      <span className="ml-auto">{modeLabel}</span>
    </footer>
  );
}
