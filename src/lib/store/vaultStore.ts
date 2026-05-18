import { create } from "zustand";
import type { LinkCandidate, TreeNode, VaultInfo } from "@/types/vault";
import {
  closeVault,
  fileTree,
  openVault,
  pickVaultFolder,
} from "@/lib/tauri/vault";
import { listLinkCandidates } from "@/lib/tauri/link";

interface VaultStore {
  info: VaultInfo | null;
  tree: TreeNode | null;
  candidates: LinkCandidate[];
  loading: boolean;
  error: string | null;

  pickAndOpen: () => Promise<void>;
  openPath: (path: string) => Promise<void>;
  close: () => Promise<void>;
  refreshTree: () => Promise<void>;
  refreshCandidates: () => Promise<void>;
  resolveWikiTarget: (target: string) => string | null;
}

export const useVaultStore = create<VaultStore>((set, get) => ({
  info: null,
  tree: null,
  candidates: [],
  loading: false,
  error: null,

  pickAndOpen: async () => {
    try {
      const path = await pickVaultFolder();
      if (!path) return;
      await get().openPath(path);
    } catch (e) {
      set({ error: messageOf(e) });
    }
  },

  openPath: async (path: string) => {
    set({ loading: true, error: null });
    try {
      const info = await openVault(path);
      const [tree, candidates] = await Promise.all([
        fileTree(),
        listLinkCandidates(),
      ]);
      set({ info, tree, candidates, loading: false });
    } catch (e) {
      set({ error: messageOf(e), loading: false });
    }
  },

  close: async () => {
    await closeVault().catch(() => {});
    set({ info: null, tree: null, candidates: [], error: null });
  },

  refreshTree: async () => {
    if (!get().info) return;
    try {
      const [tree, candidates] = await Promise.all([
        fileTree(),
        listLinkCandidates(),
      ]);
      set({ tree, candidates });
    } catch (e) {
      set({ error: messageOf(e) });
    }
  },

  refreshCandidates: async () => {
    if (!get().info) return;
    try {
      set({ candidates: await listLinkCandidates() });
    } catch (e) {
      set({ error: messageOf(e) });
    }
  },

  resolveWikiTarget: (target: string) => {
    const candidates = get().candidates;
    if (!target) return null;

    const lower = target.toLowerCase();
    const alreadyMd = lower.endsWith(".md") || lower.endsWith(".markdown");

    if (alreadyMd) {
      const hit = candidates.find((c) => c.path === target);
      if (hit) return hit.path;
    } else {
      const withMd = `${target}.md`;
      const withMarkdown = `${target}.markdown`;
      const hitPath = candidates.find(
        (c) => c.path === withMd || c.path === withMarkdown
      );
      if (hitPath) return hitPath.path;
    }

    const basename = target.split("/").pop() ?? target;
    const hitBase = candidates.find((c) => {
      const stem = c.path.replace(/\.(md|markdown)$/i, "");
      const base = stem.split("/").pop() ?? stem;
      return base === basename;
    });
    if (hitBase) return hitBase.path;

    const hitTitle = candidates.find((c) => c.title === target);
    return hitTitle?.path ?? null;
  },
}));

function messageOf(e: unknown): string {
  if (e && typeof e === "object" && "message" in e) {
    return String((e as { message: unknown }).message);
  }
  return String(e);
}
