import { invoke } from "@tauri-apps/api/core";
import type { SsmStatus, StreamingChatTurn } from "@/types/vault";

export function ssmStatus(): Promise<SsmStatus> {
  return invoke<SsmStatus>("ssm_status");
}

export function ssmReset(): Promise<SsmStatus> {
  return invoke<SsmStatus>("ssm_reset");
}

export function streamingChat(
  question: string,
  blend = 0.5,
  limit = 3
): Promise<StreamingChatTurn> {
  return invoke<StreamingChatTurn>("streaming_chat", {
    question,
    blend,
    limit,
  });
}
