import { invoke } from "@tauri-apps/api/core";
import type { RelatedNote } from "@/types/vault";

export function findRelated(path: string, limit = 10): Promise<RelatedNote[]> {
  return invoke<RelatedNote[]>("find_related", { path, limit });
}
