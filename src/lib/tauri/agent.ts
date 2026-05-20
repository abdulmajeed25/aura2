import { invoke } from "@tauri-apps/api/core";
import type {
  ApplyReport,
  LinkSuggestion,
  OrphanNote,
} from "@/types/vault";

export interface SuggestParams {
  minScore?: number;
  limitPerSource?: number;
  totalLimit?: number;
}

export function suggestLinks(p: SuggestParams = {}): Promise<LinkSuggestion[]> {
  return invoke<LinkSuggestion[]>("suggest_links", {
    minScore: p.minScore ?? null,
    limitPerSource: p.limitPerSource ?? null,
    totalLimit: p.totalLimit ?? null,
  });
}

export function findOrphanNotes(): Promise<OrphanNote[]> {
  return invoke<OrphanNote[]>("find_orphan_notes");
}

export function applyLinkSuggestion(
  sourcePath: string,
  targetPath: string,
  alias?: string
): Promise<ApplyReport> {
  return invoke<ApplyReport>("apply_link_suggestion", {
    sourcePath,
    targetPath,
    alias: alias ?? null,
  });
}
