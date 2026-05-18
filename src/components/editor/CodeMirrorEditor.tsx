"use client";

import { useEffect, useRef } from "react";
import { EditorState } from "@codemirror/state";
import { EditorView, keymap, lineNumbers, drawSelection } from "@codemirror/view";
import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
import { syntaxHighlighting, defaultHighlightStyle } from "@codemirror/language";
import { useEditorStore } from "@/lib/store/editorStore";

interface Props {
  path: string;
  initialContent: string;
}

export function CodeMirrorEditor({ path, initialContent }: Props) {
  const hostRef = useRef<HTMLDivElement | null>(null);
  const viewRef = useRef<EditorView | null>(null);

  // Re-create the editor view whenever the active file path changes,
  // so the document and history get a clean slate.
  useEffect(() => {
    if (!hostRef.current) return;

    const state = EditorState.create({
      doc: initialContent,
      extensions: [
        lineNumbers(),
        history(),
        drawSelection(),
        syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
        keymap.of([
          ...defaultKeymap,
          ...historyKeymap,
          {
            key: "Mod-s",
            preventDefault: true,
            run: () => {
              useEditorStore.getState().save();
              return true;
            },
          },
        ]),
        markdown({ base: markdownLanguage }),
        EditorView.lineWrapping,
        EditorView.updateListener.of((u) => {
          if (u.docChanged) {
            useEditorStore.getState().setContent(u.state.doc.toString());
          }
        }),
      ],
    });

    const view = new EditorView({ state, parent: hostRef.current });
    viewRef.current = view;

    return () => {
      view.destroy();
      viewRef.current = null;
    };
    // initialContent is intentionally captured on mount; subsequent edits flow
    // through Zustand → EditorView via store, not via prop changes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [path]);

  return <div ref={hostRef} className="h-full w-full overflow-hidden" />;
}
