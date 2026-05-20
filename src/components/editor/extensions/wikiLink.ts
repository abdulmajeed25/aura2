import {
  Decoration,
  DecorationSet,
  EditorView,
  MatchDecorator,
  ViewPlugin,
  ViewUpdate,
} from "@codemirror/view";
import { RangeSetBuilder } from "@codemirror/state";

const WIKI_LINK_RE = /\[\[([^\[\]\n]+?)\]\]/g;

/**
 * Decorate every `[[...]]` occurrence as a clickable mark. Clicks are routed
 * to `onClick(target)` — the surrounding app resolves the target and decides
 * whether to open it.
 */
export function wikiLinkExtension(opts: {
  onClick: (target: string) => void;
  isResolved?: (target: string) => boolean;
}) {
  const matcher = new MatchDecorator({
    regexp: WIKI_LINK_RE,
    decoration: (match) => {
      const inner = match[1];
      const target = inner.split("|")[0].split("#")[0].trim();
      const resolved = opts.isResolved?.(target) ?? true;
      return Decoration.mark({
        class: resolved ? "cm-wiki-link" : "cm-wiki-link cm-wiki-link-unresolved",
        attributes: { "data-wiki-target": target },
      });
    },
  });

  const plugin = ViewPlugin.fromClass(
    class {
      decorations: DecorationSet;

      constructor(view: EditorView) {
        this.decorations = matcher.createDeco(view);
      }

      update(update: ViewUpdate) {
        if (update.docChanged || update.viewportChanged) {
          this.decorations = matcher.updateDeco(update, this.decorations);
        }
      }
    },
    {
      decorations: (v) => v.decorations,
      eventHandlers: {
        mousedown(e: MouseEvent) {
          const target = e.target as HTMLElement | null;
          const el = target?.closest?.("[data-wiki-target]") as HTMLElement | null;
          if (!el) return false;
          const value = el.getAttribute("data-wiki-target");
          if (!value) return false;
          // Allow ctrl/cmd-click to take the user to the link without
          // disturbing text selection on a plain click.
          if (!(e.metaKey || e.ctrlKey)) return false;
          e.preventDefault();
          opts.onClick(value);
          return true;
        },
      },
    }
  );

  return plugin;
}

/**
 * Re-using the same regex but emitted as a stateless extension is occasionally
 * useful in tests.
 */
export function buildWikiLinkDecorations(content: string): DecorationSet {
  const builder = new RangeSetBuilder<Decoration>();
  for (const m of content.matchAll(WIKI_LINK_RE)) {
    if (m.index === undefined) continue;
    const target = m[1].split("|")[0].split("#")[0].trim();
    builder.add(
      m.index,
      m.index + m[0].length,
      Decoration.mark({
        class: "cm-wiki-link",
        attributes: { "data-wiki-target": target },
      })
    );
  }
  return builder.finish();
}
