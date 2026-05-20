import {
  Decoration,
  DecorationSet,
  EditorView,
  ViewPlugin,
  ViewUpdate,
  WidgetType,
} from "@codemirror/view";
import { RangeSetBuilder } from "@codemirror/state";
import { EmbedCache, parseRefInner } from "@/lib/embeds";
import { renderMarkdown } from "@/lib/markdown";

class EmbedWidget extends WidgetType {
  constructor(
    private readonly target: string,
    private readonly heading: string | null,
    private readonly blockRef: string | null,
    private readonly cache: EmbedCache,
    private readonly onUpdate: () => void
  ) {
    super();
  }

  eq(other: EmbedWidget): boolean {
    return (
      this.target === other.target &&
      this.heading === other.heading &&
      this.blockRef === other.blockRef
    );
  }

  toDOM(): HTMLElement {
    const host = document.createElement("div");
    host.className = "cm-embed-widget";
    host.contentEditable = "false";
    host.dataset.embedTarget = this.target;
    host.innerHTML = `<div class="cm-embed-loading">Loading ${this.target}…</div>`;

    void this.cache
      .get({
        target: this.target,
        heading: this.heading,
        blockRef: this.blockRef,
        display: null,
      })
      .then((result) => {
        if (!result) {
          host.innerHTML = `<div class="cm-embed-missing">${escape(this.target)} not found</div>`;
          this.onUpdate();
          return;
        }
        const headerText = this.blockRef
          ? `${result.source_title} • ^${this.blockRef}`
          : this.heading
            ? `${result.source_title} • ${this.heading}`
            : result.source_title;
        host.innerHTML = `<header class="cm-embed-header">${escape(headerText)}</header><div class="cm-embed-body aura-prose">${renderMarkdown(result.content)}</div>`;
        this.onUpdate();
      });

    return host;
  }

  ignoreEvent(): boolean {
    // Allow clicks inside the widget (e.g. wiki-links).
    return false;
  }
}

function escape(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

const EMBED_RE = /!\[\[([^\[\]\n]+?)\]\]/g;

/**
 * In Live Preview mode, render each `![[...]]` line with the embedded
 * content shown as a block widget directly after the line.
 */
export function transclusionExtension() {
  return ViewPlugin.fromClass(
    class {
      decorations: DecorationSet;
      cache = new EmbedCache();

      constructor(view: EditorView) {
        this.decorations = this.build(view);
      }

      update(update: ViewUpdate) {
        if (update.docChanged || update.viewportChanged) {
          this.decorations = this.build(update.view);
        }
      }

      build(view: EditorView): DecorationSet {
        const builder = new RangeSetBuilder<Decoration>();
        for (const { from, to } of view.visibleRanges) {
          const text = view.state.doc.sliceString(from, to);
          for (const m of text.matchAll(EMBED_RE)) {
            if (m.index === undefined) continue;
            const matchStart = from + m.index;
            const matchEnd = matchStart + m[0].length;
            const ref = parseRefInner(m[1]);
            if (!ref.target) continue;

            const line = view.state.doc.lineAt(matchEnd);
            const widget = Decoration.widget({
              widget: new EmbedWidget(
                ref.target,
                ref.heading,
                ref.blockRef,
                this.cache,
                () => view.requestMeasure()
              ),
              block: true,
              side: 1,
            });
            builder.add(line.to, line.to, widget);
          }
        }
        return builder.finish();
      }
    },
    {
      decorations: (v) => v.decorations,
    }
  );
}
