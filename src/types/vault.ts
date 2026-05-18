export interface VaultInfo {
  root: string;
  file_count: number;
}

export interface FileEntry {
  path: string;
  name: string;
  is_dir: boolean;
  size: number;
  modified_at: number;
}

export interface TreeNode {
  path: string;
  name: string;
  is_dir: boolean;
  children: TreeNode[];
}

export interface ReindexReport {
  indexed: number;
  skipped: number;
}

export interface VaultChangeEvent {
  kind: "created" | "modified" | "removed" | "renamed" | "other";
  path: string;
}

export interface AuraError {
  code: string;
  message: string;
}
