import { invoke } from "@tauri-apps/api/core";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import type {
  FileEntry,
  ReindexReport,
  TreeNode,
  VaultInfo,
} from "@/types/vault";

export async function pickVaultFolder(): Promise<string | null> {
  const result = await openDialog({
    directory: true,
    multiple: false,
    title: "Select Vault Folder",
  });
  return (result as string | null) ?? null;
}

export function openVault(path: string): Promise<VaultInfo> {
  return invoke<VaultInfo>("open_vault", { path });
}

export function closeVault(): Promise<void> {
  return invoke<void>("close_vault");
}

export function currentVault(): Promise<VaultInfo | null> {
  return invoke<VaultInfo | null>("current_vault");
}

export function reindexVault(): Promise<ReindexReport> {
  return invoke<ReindexReport>("reindex_vault");
}

export function listFiles(): Promise<FileEntry[]> {
  return invoke<FileEntry[]>("list_files");
}

export function fileTree(): Promise<TreeNode> {
  return invoke<TreeNode>("file_tree");
}
