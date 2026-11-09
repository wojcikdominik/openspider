import { Fragment, type ReactNode } from "react";
import { marked, type Token, type Tokens } from "marked";
import { openInBrowser } from "../help/feedback";

/**
 * Shared building block for release notes (OUT-251 update dialog, OUT-257 "What's new"): one splitter and one safe
 * Markdown renderer. Notes are English first, then the complete German list under a line `### Deutsch`
 * (docs/release/release-rhythmus.md).
 */
export interface NoteLanguages {
  en: string;
  de: string;
}

/** The marker line between the English and the German part. */
export const GERMAN_MARKER = "### Deutsch";

/**
 * Splits bilingual notes at the first line that is exactly `### Deutsch` (surrounding blanks ignored). Without the
 * marker, or with an empty German part, both languages get the whole text, so older single-language notes stay usable.
 */
export function splitNotes(source: string): NoteLanguages {
  const lines = source.replaceAll("\r\n", "\n").split("\n");
  const index = lines.findIndex(line => line.trim() === GERMAN_MARKER);
  if (index < 0) {
    const all = lines.join("\n").trim();
    return { en: all, de: all };
  }
  const en = lines.slice(0, index).join("\n").trim();
  const de = lines.slice(index + 1).join("\n").trim();
  return { en, de: de || en };
}

/** OUT-257 language rule, also used by the update dialog: German UI → German part, every other UI language → English. */
export function notesForLanguage(source: string, lang: string): string {
  const parts = splitNotes(source);
  return lang === "de" || lang.startsWith("de-") ? parts.de : parts.en;
}

/** Only absolute http(s) links are offered; everything else is shown as its text. */
function safeHref(href: string): string | null {
  return /^https?:\/\//i.test(href) ? href : null;
}

/**
 * Renders release notes from Markdown with React text nodes and an explicit element allowlist: raw HTML is shown as
 * escaped text, images never enter the DOM, links open in the default browser (OUT-251, OUT-257).
 */
export function NoteMarkdown({ source, headingLevel = 4 }: { source: string; headingLevel?: 3 | 4 | 5 }) {
  const Heading = `h${headingLevel}` as const;
  function render(tokens: Token[] = []): ReactNode {
    return tokens.map((token, index) => {
      let content: ReactNode;
      switch (token.type) {
        case "paragraph": content = <p>{render(token.tokens ?? [])}</p>; break;
        case "text": content = token.tokens ? render(token.tokens) : token.text; break;
        case "strong": content = <strong>{render(token.tokens)}</strong>; break;
        case "em": content = <em>{render(token.tokens)}</em>; break;
        case "codespan": content = <code>{token.text}</code>; break;
        case "code": content = <pre><code>{token.text}</code></pre>; break;
        case "br": content = <br />; break;
        case "heading": content = <Heading>{render(token.tokens)}</Heading>; break;
        case "blockquote": content = <blockquote>{render(token.tokens)}</blockquote>; break;
        case "list": {
          const items = token.items.map((item: Tokens.ListItem, i: number) => <li key={i}>{render(item.tokens)}</li>);
          content = token.ordered ? <ol start={token.start || 1}>{items}</ol> : <ul>{items}</ul>;
          break;
        }
        case "link": {
          const href = safeHref(token.href);
          content = href
            ? <a href={href} onClick={event => { event.preventDefault(); void openInBrowser(href).catch(() => {}); }}>{render(token.tokens)}</a>
            : render(token.tokens);
          break;
        }
        case "del": content = <del>{render(token.tokens)}</del>; break;
        case "escape": content = token.text; break;
        case "image": content = token.text; break;
        case "space": content = null; break;
        default: content = token.raw; // Includes HTML: escaped by React.
      }
      return <Fragment key={index}>{content}</Fragment>;
    });
  }
  return <>{render(marked.lexer(source))}</>;
}
