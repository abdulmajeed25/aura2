import { invoke } from "@tauri-apps/api/core";
import type { CanvasDoc, CanvasFile } from "@/types/vault";

export function listCanvases(): Promise<CanvasFile[]> {
  return invoke<CanvasFile[]>("list_canvases");
}

export function readCanvas(path: string): Promise<CanvasDoc> {
  return invoke<CanvasDoc>("read_canvas", { path });
}

export function writeCanvas(path: string, doc: CanvasDoc): Promise<void> {
  return invoke<void>("write_canvas", { path, doc });
}

export function createCanvas(path: string): Promise<void> {
  return invoke<void>("create_canvas", { path });
}
