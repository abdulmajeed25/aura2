import {
  autocompletion,
  CompletionContext,
  CompletionResult,
} from "@codemirror/autocomplete";
import type { LinkCandidate } from "@/types/vault";

/**
 * `[[` autocomplete that suggests vault notes. The caller passes a getter
 * because the candidate list changes as the user adds/removes files.
 */
export function wikiAutocompleteExtension(
  getCandidates: () => LinkCandidate[]
) {
  return autocompletion({
    override: [
      (ctx: CompletionContext): CompletionResult | null => {
        // Look back from the cursor for an unclosed `[[`.
        const line = ctx.state.doc.lineAt(ctx.pos);
        const upToCursor = line.text.slice(0, ctx.pos - line.from);
        const open = upToCursor.lastIndexOf("[[");
        if (open === -1) return null;
        // If a `]]` appeared after the last `[[`, we're outside an open link.
        const close = upToCursor.indexOf("]]", open);
        if (close !== -1) return null;

        const queryStart = line.from + open + 2;
        const query = ctx.state.doc.sliceString(queryStart, ctx.pos);
        if (/[\n\]]/.test(query)) return null;

        const candidates = getCandidates();
        const lower = query.toLowerCase();
        const matches = candidates
          .filter((c) => {
            if (!lower) return true;
            const stem = c.path.replace(/\.(md|markdown)$/i, "");
            return (
              stem.toLowerCase().includes(lower) ||
              c.title.toLowerCase().includes(lower)
            );
          })
          .slice(0, 50);

        return {
          from: queryStart,
          to: ctx.pos,
          options: matches.map((c) => {
            const stem = c.path.replace(/\.(md|markdown)$/i, "");
            return {
              label: stem,
              displayLabel: c.title,
              detail: stem,
              apply: `${stem}]]`,
              type: "file",
            };
          }),
          validFor: /^[^\]\n#|]*$/,
        };
      },
    ],
  });
}
