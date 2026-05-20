import { invoke } from "@tauri-apps/api/core";
import type {
  MediaRow,
  MediaScanReport,
  MediaToolsStatus,
} from "@/types/vault";

export function mediaToolsStatus(): Promise<MediaToolsStatus> {
  return invoke<MediaToolsStatus>("media_tools_status");
}

export function ingestMedia(path: string): Promise<MediaRow> {
  return invoke<MediaRow>("ingest_media", { path });
}

export function scanMedia(): Promise<MediaScanReport> {
  return invoke<MediaScanReport>("scan_media");
}

export function listMedia(): Promise<MediaRow[]> {
  return invoke<MediaRow[]>("list_media");
}

export function deleteMedia(path: string): Promise<void> {
  return invoke<void>("delete_media", { path });
}
