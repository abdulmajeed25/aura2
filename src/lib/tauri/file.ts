import { invoke } from "@tauri-apps/api/core";

export function readFile(path: string): Promise<string> {
  return invoke<string>("read_file", { path });
}

export function writeFile(path: string, content: string): Promise<void> {
  return invoke<void>("write_file", { path, content });
}

export function createFile(
  path: string,
  initialContent?: string
): Promise<void> {
  return invoke<void>("create_file", { path, initialContent });
}

export function deleteFile(path: string): Promise<void> {
  return invoke<void>("delete_file", { path });
}

export function renameFile(fromPath: string, toPath: string): Promise<void> {
  return invoke<void>("rename_file", { fromPath, toPath });
}
