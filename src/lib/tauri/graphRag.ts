import { invoke } from "@tauri-apps/api/core";
import type { GraphRagAnswer, GraphRagRebuildReport } from "@/types/vault";

export function rebuildGraphRag(): Promise<GraphRagRebuildReport> {
  return invoke<GraphRagRebuildReport>("rebuild_graph_rag");
}

export function graphRagQuery(
  question: string,
  limit = 3
): Promise<GraphRagAnswer> {
  return invoke<GraphRagAnswer>("graph_rag_query", { question, limit });
}
