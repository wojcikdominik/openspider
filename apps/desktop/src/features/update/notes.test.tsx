import { afterEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import * as feedback from "../help/feedback";
import { NoteMarkdown, notesForLanguage, splitNotes } from "./notes";
import beta3 from "../../../../../tests/fixtures/update-notes/1.0.0-beta.3.md?raw";
import short from "../../../../../tests/fixtures/update-notes/short.md?raw";

afterEach(() => { cleanup(); vi.restoreAllMocks(); });

// spec: OUT-251
// spec: OUT-257
it("splits the real beta.3 notes at the German marker: German UI gets the German part, en and es the English part", () => {
  const { en, de } = splitNotes(beta3);
  expect(en.startsWith("Faster crawls, Chromium on demand")).toBe(true);
  expect(en).not.toContain("### Deutsch");
  expect(en).not.toContain("Schnellere Crawls");
  expect(de.startsWith("Schnellere Crawls, Chromium auf Abruf")).toBe(true);
  expect(de).not.toContain("Faster crawls");
  expect(notesForLanguage(beta3, "de")).toBe(de);
  expect(notesForLanguage(beta3, "en")).toBe(en);
  expect(notesForLanguage(beta3, "es")).toBe(en);
  expect(notesForLanguage(beta3, "fr")).toBe(en);
  expect(notesForLanguage(beta3, "it")).toBe(en);
  // Windows line endings and notes without German part (older releases) stay usable.
  expect(splitNotes(beta3.replaceAll("\n", "\r\n"))).toEqual({ en, de });
  expect(splitNotes("- only English\n")).toEqual({ en: "- only English", de: "- only English" });
  expect(splitNotes("- English\n\n### Deutsch\n\n  \n")).toEqual({ en: "- English", de: "- English" });
  // Only a line that is exactly the marker splits; the word inside a sentence does not.
  expect(splitNotes("- mentions ### Deutsch inline\n")).toEqual({ en: "- mentions ### Deutsch inline", de: "- mentions ### Deutsch inline" });
});

// spec: OUT-251
it("renders the beta.3 list as list items in de, en and es instead of raw Markdown", () => {
  for (const [lang, first, count] of [["de", "Große Crawls schneller", 11], ["en", "Faster large crawls", 11], ["es", "Faster large crawls", 11]] as const) {
    const { container } = render(<NoteMarkdown source={notesForLanguage(beta3, lang)} />);
    const items = container.querySelectorAll("ul > li");
    expect(items, lang).toHaveLength(count);
    expect(items[0]!.textContent, lang).toContain(first);
    expect(container.textContent, lang).not.toContain("### Deutsch");
    expect(container.textContent, lang).not.toMatch(/^- /m);
    cleanup();
  }
});

// spec: OUT-251
it("renders bold text and opens http(s) links in the default browser", () => {
  const browser = vi.spyOn(feedback, "openInBrowser").mockResolvedValue();
  render(<NoteMarkdown source={notesForLanguage(short, "de")} />);
  expect(screen.getByText("Update-Dialog").tagName).toBe("STRONG");
  const link = screen.getByRole("link", { name: "Handbuch" });
  fireEvent.click(link);
  expect(browser).toHaveBeenCalledWith("https://openspider.de/handbuch/");
});

// spec: OUT-251
it("never executes HTML: scripts, images, event handlers and javascript: links stay text", () => {
  const source = [
    "- **Bold** <script>window.__pwned = 1</script>",
    "- [bad](javascript:alert(1)) and [data](data:text/html,x)",
    "- ![tracker](https://bad.invalid/pixel.png)",
    "",
    '<img src="https://bad.invalid/x" onerror="window.__pwned = 2">',
    "",
    "<iframe src=\"https://bad.invalid\"></iframe>",
  ].join("\n");
  const { container } = render(<NoteMarkdown source={source} />);
  expect(container.querySelector("script, img, iframe, [onerror]")).toBeNull();
  expect(container.querySelectorAll("a")).toHaveLength(0);
  expect(container.textContent).toContain("<script>");
  expect(container.textContent).toContain("bad");
  expect((window as unknown as { __pwned?: number }).__pwned).toBeUndefined();
});
