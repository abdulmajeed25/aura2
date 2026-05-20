import { invoke } from "@tauri-apps/api/core";
import type { GraphSnapshot } from "@/types/vault";

export interface GraphParams {
  iterations?: number;
  width?: number;
  height?: number;
}

export function getGraphSnapshot(params: GraphParams = {}): Promise<GraphSnapshot> {
  return invoke<GraphSnapshot>("get_graph_snapshot", {
    iterations: params.iterations ?? null,
    width: params.width ?? null,
    height: params.height ?? null,
  });
}
