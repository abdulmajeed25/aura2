"use client";

import { useEffect, useRef } from "react";
import { EditorState } from "@codemirror/state";
import { EditorView, keymap, lineNumbers, drawSelection } from "@codemirror/view";
import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
import { syntaxHighlighting, defaultHighlightStyle } from "@codemirror/language";
import {
  completionKeymap,
  startCompletion,
} from "@codemirror/autocomplete";
import { useEditorStore } from "@/lib/store/editorStore";
import { useVaultStore } from "@/lib/store/vaultStore";
import { wikiLinkExtension } from "./extensions/wikiLink";
import { wikiAutocompleteExtension } from "./extensions/wikiAutocomplete";
import { transclusionExtension } from "./extensions/transclusion";

interface Props {
  path: string;
  initialContent: string;
  liveTransclusion?: boolean;
}

export function CodeMirrorEditor({ path, initialContent, liveTransclusion = false }: Props) {
  const hostRef = useRef<HTMLDivElement | null>(null);
  const viewRef = useRef<EditorView | null>(null);

  useEffect(() => {
    if (!hostRef.current) return;

    const baseExtensions = [
      lineNumbers(),
      history(),
      drawSelection(),
      syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
      markdown({ base: markdownLanguage }),
      EditorView.lineWrapping,
      wikiLinkExtension({
        onClick: (target) => {
          const resolved = useVaultStore.getState().resolveWikiTarget(target);
          if (resolved) {
            void useEditorStore.getState().openFile(resolved);
          }
        },
        isResolved: (target) =>
          useVaultStore.getState().resolveWikiTarget(target) !== null,
      }),
      wikiAutocompleteExtension(() => useVaultStore.getState().candidates),
      keymap.of([
        ...defaultKeymap,
        ...historyKeymap,
        ...completionKeymap,
        {
          key: "Mod-s",
          preventDefault: true,
          run: () => {
            useEditorStore.getState().save();
            return true;
          },
        },
      ]),
      EditorView.updateListener.of((u) => {
        if (u.docChanged) {
          useEditorStore.getState().setContent(u.state.doc.toString());
          const lastTwo = u.state.doc.sliceString(
            Math.max(0, u.state.selection.main.head - 2),
            u.state.selection.main.head
          );
          if (lastTwo === "[[") {
            startCompletion(u.view);
          }
        }
      }),
    ];

    const state = EditorState.create({
      doc: initialContent,
      extensions: liveTransclusion
        ? [...baseExtensions, transclusionExtension()]
        : baseExtensions,
    });

    const view = new EditorView({ state, parent: hostRef.current });
    viewRef.current = view;

    return () => {
      view.destroy();
      viewRef.current = null;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [path, liveTransclusion]);

  return <div ref={hostRef} className="h-full w-full overflow-hidden" />;
}
