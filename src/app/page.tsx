"use client";

import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { FolderOpen, RefreshCw, Save } from "lucide-react";
import { CodeMirrorEditor } from "@/components/editor/CodeMirrorEditor";
import { FileExplorer } from "@/components/sidebar/FileExplorer";
import { useVaultStore } from "@/lib/store/vaultStore";
import { useEditorStore } from "@/lib/store/editorStore";
import { reindexVault } from "@/lib/tauri/vault";
import type { VaultChangeEvent } from "@/types/vault";

export default function HomePage() {
  const { info, tree, loading, error, pickAndOpen, refreshTree } =
    useVaultStore();
  const { activePath, content, dirty, saving, save } = useEditorStore();

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    listen<VaultChangeEvent>("vault://changed", (event) => {
      void useVaultStore.getState().refreshTree();
      const editor = useEditorStore.getState();
      if (event.payload.kind === "removed" && editor.activePath === event.payload.path) {
        editor.close();
      }
    }).then((u) => {
      unlisten = u;
    });
    return () => {
      unlisten?.();
    };
  }, []);

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
        </aside>
        <main className="flex-1 min-w-0 flex flex-col bg-[var(--color-bg)]">
          {activePath ? (
            <>
              <div className="px-4 py-2 border-b border-[var(--color-border)] flex items-center gap-3 text-xs text-[var(--color-text-dim)]">
                <span className="truncate">{activePath}</span>
                {dirty && <span className="text-amber-400">●</span>}
                <button
                  type="button"
                  onClick={() => void save()}
                  disabled={!dirty || saving}
                  className="ml-auto inline-flex items-center gap-1 px-2 py-1 rounded hover:bg-[var(--color-surface-hover)] disabled:opacity-40"
                  title="Save (Ctrl/Cmd+S)"
                >
                  <Save size={12} />
                  {saving ? "Saving…" : "Save"}
                </button>
              </div>
              <div className="flex-1 min-h-0">
                <CodeMirrorEditor path={activePath} initialContent={content} />
              </div>
            </>
          ) : (
            <div className="flex-1 flex items-center justify-center text-[var(--color-text-faint)] text-sm">
              Select a note from the sidebar to start editing.
            </div>
          )}
        </main>
      </div>
      <StatusBar />
    </div>
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

function StatusBar() {
  const info = useVaultStore((s) => s.info);
  const activePath = useEditorStore((s) => s.activePath);
  return (
    <footer className="h-6 shrink-0 border-t border-[var(--color-border)] flex items-center px-3 text-[11px] text-[var(--color-text-faint)] gap-4">
      <span>{info?.file_count ?? 0} notes indexed</span>
      {activePath && <span className="truncate">{activePath}</span>}
    </footer>
  );
}
