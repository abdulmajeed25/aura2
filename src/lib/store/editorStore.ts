import { create } from "zustand";
import { readFile, writeFile } from "@/lib/tauri/file";

interface EditorStore {
  activePath: string | null;
  content: string;
  dirty: boolean;
  saving: boolean;
  error: string | null;

  openFile: (path: string) => Promise<void>;
  setContent: (content: string) => void;
  save: () => Promise<void>;
  close: () => void;
}

export const useEditorStore = create<EditorStore>((set, get) => ({
  activePath: null,
  content: "",
  dirty: false,
  saving: false,
  error: null,

  openFile: async (path) => {
    set({ error: null });
    try {
      const content = await readFile(path);
      set({ activePath: path, content, dirty: false });
    } catch (e) {
      set({ error: messageOf(e) });
    }
  },

  setContent: (content) => {
    set({ content, dirty: true });
  },

  save: async () => {
    const { activePath, content, dirty } = get();
    if (!activePath || !dirty) return;
    set({ saving: true, error: null });
    try {
      await writeFile(activePath, content);
      set({ dirty: false, saving: false });
    } catch (e) {
      set({ error: messageOf(e), saving: false });
    }
  },

  close: () => {
    set({ activePath: null, content: "", dirty: false });
  },
}));

function messageOf(e: unknown): string {
  if (e && typeof e === "object" && "message" in e) {
    return String((e as { message: unknown }).message);
  }
  return String(e);
}
