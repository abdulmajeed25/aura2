import { invoke } from "@tauri-apps/api/core";
import type {
  BacklinkEntry,
  HeadingEntry,
  LinkCandidate,
  OutgoingLinkEntry,
} from "@/types/vault";

export function getBacklinks(path: string): Promise<BacklinkEntry[]> {
  return invoke<BacklinkEntry[]>("get_backlinks", { path });
}

export function getOutgoingLinks(path: string): Promise<OutgoingLinkEntry[]> {
  return invoke<OutgoingLinkEntry[]>("get_outgoing_links", { path });
}

export function getOutline(path: string): Promise<HeadingEntry[]> {
  return invoke<HeadingEntry[]>("get_outline", { path });
}

export function listLinkCandidates(): Promise<LinkCandidate[]> {
  return invoke<LinkCandidate[]>("list_link_candidates");
}
