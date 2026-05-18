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

export interface BacklinkEntry {
  source_file_id: string;
  source_path: string;
  source_title: string;
  link_text: string;
  display_text: string | null;
  line_number: number;
  context: string | null;
}

export interface OutgoingLinkEntry {
  link_text: string;
  display_text: string | null;
  target_path: string | null;
  target_title: string | null;
  target_heading: string | null;
  target_block_ref: string | null;
  line_number: number;
  column_number: number;
  is_resolved: boolean;
}

export interface HeadingEntry {
  level: number;
  text: string;
  line_number: number;
}

export interface LinkCandidate {
  path: string;
  title: string;
}

export type EmbedKind = "file" | "heading" | "block";

export interface EmbedResult {
  kind: EmbedKind;
  source_path: string;
  source_title: string;
  content: string;
}

export interface GraphNode {
  id: string;
  path: string;
  title: string;
  x: number;
  y: number;
  degree: number;
}

export interface GraphEdge {
  source: string;
  target: string;
}

export interface GraphSnapshot {
  nodes: GraphNode[];
  edges: GraphEdge[];
  iterations: number;
}

export type SearchMode = "semantic" | "fts" | "hybrid";

export interface SearchHit {
  block_id: string;
  file_id: string;
  file_path: string;
  file_title: string;
  block_type: string;
  line_number: number;
  score: number;
  snippet: string;
  matched_via: "semantic" | "fts" | "both";
}
