import { describe, expect, it } from "vitest";
import { catalogues, ptBR } from "./pt-BR";

// The source of every screen, read as text: what is not a test, a catalogue or the layer itself.
const sources = import.meta.glob<string>(["../**/*.ts", "../**/*.tsx", "!../**/*.test.ts", "!../**/*.test.tsx", "!./pt-BR/**", "!./index.tsx"], {
  query: "?raw",
  import: "default",
  eager: true,
});

/** A text written to be translated: `t("…")`, `rich("…")` or, where no component can, `msg("…")`. */
const MARK = /\b(?:t|rich|msg)\(\s*"((?:[^"\\]|\\.)*)"/g;

const used = new Map<string, string>();
for (const [file, text] of Object.entries(sources)) {
  for (const match of text.matchAll(MARK)) {
    used.set(JSON.parse(`"${match[1]}"`) as string, file);
  }
}

const placeholders = (text: string) => [...text.matchAll(/\{(\w+)\}/g)].map((match) => match[1]).sort();

describe("the Portuguese catalogue", () => {
  it("reads the sources (a broken glob would make every other check pass for nothing)", () => {
    expect(Object.keys(sources).length).toBeGreaterThan(50);
    expect(used.size).toBeGreaterThan(50);
  });

  it("translates every text the screens are written in", () => {
    const missing = [...used].filter(([key]) => !(key in ptBR)).map(([key, file]) => `${key}  (${file})`);
    expect(missing).toEqual([]);
  });

  it("holds nothing the screens no longer use", () => {
    expect(Object.keys(ptBR).filter((key) => !used.has(key))).toEqual([]);
  });

  it("gives every key once, and a translation that is something", () => {
    const keys = Object.values(catalogues).flatMap((catalogue) => Object.keys(catalogue));
    expect(keys.filter((key, index) => keys.indexOf(key) !== index)).toEqual([]);
    expect(Object.entries(ptBR).filter(([, text]) => text.trim() === "")).toEqual([]);
  });

  it("keeps the parts to fill in: a translation has the same {names} as its source", () => {
    const different = Object.entries(ptBR).filter(([key, text]) => placeholders(key).join() !== placeholders(text).join());
    expect(different).toEqual([]);
  });
});
