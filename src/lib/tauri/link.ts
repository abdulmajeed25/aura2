import { invoke } from "@tauri-apps/api/core";
import type {
  BacklinkEntry,
  EmbedResult,
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

export function resolveEmbed(
  target: string,
  heading: string | null,
  blockRef: string | null
): Promise<EmbedResult | null> {
  return invoke<EmbedResult | null>("resolve_embed", {
    target,
    heading,
    blockRef,
  });
}
