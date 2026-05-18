import { create } from "zustand";
import type { TreeNode, VaultInfo } from "@/types/vault";
import {
  closeVault,
  fileTree,
  openVault,
  pickVaultFolder,
} from "@/lib/tauri/vault";

interface VaultStore {
  info: VaultInfo | null;
  tree: TreeNode | null;
  loading: boolean;
  error: string | null;

  pickAndOpen: () => Promise<void>;
  openPath: (path: string) => Promise<void>;
  close: () => Promise<void>;
  refreshTree: () => Promise<void>;
}

export const useVaultStore = create<VaultStore>((set, get) => ({
  info: null,
  tree: null,
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
      const tree = await fileTree();
      set({ info, tree, loading: false });
    } catch (e) {
      set({ error: messageOf(e), loading: false });
    }
  },

  close: async () => {
    await closeVault().catch(() => {});
    set({ info: null, tree: null, error: null });
  },

  refreshTree: async () => {
    if (!get().info) return;
    try {
      const tree = await fileTree();
      set({ tree });
    } catch (e) {
      set({ error: messageOf(e) });
    }
  },
}));

function messageOf(e: unknown): string {
  if (e && typeof e === "object" && "message" in e) {
    return String((e as { message: unknown }).message);
  }
  return String(e);
}
