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

export interface RelatedNote {
  file_id: string;
  path: string;
  title: string;
  score: number;
  shared_neighbours: number;
}

export interface CommunityHit {
  community_id: number;
  level: number;
  member_count: number;
  member_paths: string[];
  member_titles: string[];
  summary_text: string;
  score: number;
}

export interface GraphRagAnswer {
  question: string;
  communities: CommunityHit[];
  context_payload: string;
  estimated_tokens: number;
  covered_notes: number;
}

export interface GraphRagRebuildReport {
  communities: number;
  members_total: number;
  avg_members: number;
}

export interface SsmStatus {
  dim: number;
  step_count: number;
  saturation: number;
  last_input_alignment: number;
  active: boolean;
}

export interface StreamingChatTurn {
  answer: GraphRagAnswer;
  status: SsmStatus;
}

export type MediaKind = "audio" | "video" | "image";

export interface MediaRow {
  id: string;
  path: string;
  kind: MediaKind;
  size_bytes: number;
  duration_ms: number | null;
  description: string;
  indexed_at: number;
}

export interface MediaScanReport {
  ingested: number;
  skipped: number;
}

export interface MediaToolsStatus {
  yt_dlp: string | null;
  ffmpeg: string | null;
  ffprobe: string | null;
}

export interface McpStatus {
  running: boolean;
  url: string | null;
  port: number | null;
  auth_token: string | null;
  started_at: number | null;
  request_count: number;
}
