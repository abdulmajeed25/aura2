import { Marked, type Tokens } from "marked";

/**
 * Marked instance configured for Aura's Reading view. Wiki-links `[[Note]]`
 * are turned into `<a class="wiki-link" data-wiki-target="...">` so the
 * surrounding React layer can attach a click handler that opens the target.
 */
function buildRenderer(): Marked {
  const m = new Marked({
    gfm: true,
    breaks: false,
  });

  m.use({
    extensions: [
      {
        name: "wikiLink",
        level: "inline",
        start(src: string) {
          return src.indexOf("[[");
        },
        tokenizer(src: string) {
          const match = /^\[\[([^\[\]\n]+?)\]\]/.exec(src);
          if (!match) return undefined;
          const inner = match[1];
          const [lhs, display] = inner.includes("|")
            ? [inner.slice(0, inner.indexOf("|")), inner.slice(inner.indexOf("|") + 1).trim()]
            : [inner, null];
          const target = lhs.split("#")[0].trim();
          return {
            type: "wikiLink",
            raw: match[0],
            target,
            display: display ?? lhs.trim(),
          };
        },
        renderer(token: Tokens.Generic) {
          const t = token as Tokens.Generic & { target: string; display: string };
          const target = String(t.target ?? "").replace(/"/g, "&quot;");
          const display = String(t.display ?? target);
          return `<a href="#" class="wiki-link" data-wiki-target="${target}">${display}</a>`;
        },
      },
    ],
  });

  return m;
}

let cached: Marked | null = null;

function getRenderer(): Marked {
  if (!cached) cached = buildRenderer();
  return cached;
}

/** Render Markdown (already expanded for embeds) to an HTML string. */
export function renderMarkdown(content: string): string {
  return getRenderer().parse(content, { async: false }) as string;
}
