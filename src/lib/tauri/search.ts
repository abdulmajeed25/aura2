import { invoke } from "@tauri-apps/api/core";
import type { SearchHit, SearchMode } from "@/types/vault";

export function searchVault(
  query: string,
  mode: SearchMode = "hybrid",
  limit = 20
): Promise<SearchHit[]> {
  return invoke<SearchHit[]>("search_vault", { query, mode, limit });
}
