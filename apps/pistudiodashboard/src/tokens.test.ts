import { readdirSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { describe, expect, it } from "vitest";

// The design tokens are the only place a colour is written, every `var(--pi-…)` a style uses is
// declared, both themes declare the same names, and the ported values equal their pin. A token typo
// must fail here, because in the browser it silently falls back to an inherited or initial value.
// (pi-studio's own `token-integrity.test.ts` guards the same failure.)
const src = path.dirname(fileURLToPath(import.meta.url));
const tokensCss = path.join(src, "tokens.css");

// ---- helpers ------------------------------------------------------------------------------

/** Style sheets and components (an inline `style` counts too), minus tests and the token file. */
function sources(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const file = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      return sources(file);
    }
    const checked = /\.(css|ts|tsx)$/.test(entry.name) && !/\.test\.tsx?$/.test(entry.name) && file !== tokensCss;
    return checked ? [file] : [];
  });
}

function stripComments(text: string): string {
  return text.replace(/\/\*[\s\S]*?\*\//g, "").replace(/(^|[^:"'`])\/\/[^\n]*/g, "$1");
}

const literal = /#[0-9a-fA-F]{3,8}\b|\b(?:rgb|rgba|hsl|hsla|oklch|oklab|lab|lch|color)\(/;

/** Lines holding a colour written as a literal. */
export function colourLiterals(text: string): string[] {
  return stripComments(text)
    .split("\n")
    .filter((line) => literal.test(line));
}

/** Every `--pi-*` name read through `var()`. */
export function tokenRefs(text: string): string[] {
  return [...stripComments(text).matchAll(/var\(\s*(--pi-[\w.-]*)/g)].map((match) => match[1]);
}

/** Refs that no declaration backs. */
export function undeclared(text: string, declared: ReadonlySet<string>): string[] {
  return tokenRefs(text).filter((name) => !declared.has(name));
}

/** The contents of each `color-mix(…)`, found by balancing parentheses. */
function colorMixes(text: string): string[] {
  const code = stripComments(text);
  const found: string[] = [];
  for (let at = code.indexOf("color-mix("); at !== -1; at = code.indexOf("color-mix(", at + 1)) {
    let depth = 0;
    for (let i = at + "color-mix".length; i < code.length; i++) {
      if (code[i] === "(") {
        depth++;
      } else if (code[i] === ")" && --depth === 0) {
        found.push(code.slice(at + "color-mix(".length, i));
        break;
      }
    }
  }
  return found;
}

/** Operands of `color-mix(in srgb, A p%, B)` that are not a `var(--pi-…)` token or `transparent`. */
export function badMixOperands(text: string): string[] {
  const bad: string[] = [];
  for (const mix of colorMixes(text)) {
    const parts: string[] = [];
    let depth = 0;
    let start = 0;
    for (let i = 0; i < mix.length; i++) {
      if (mix[i] === "(") {
        depth++;
      } else if (mix[i] === ")") {
        depth--;
      } else if (mix[i] === "," && depth === 0) {
        parts.push(mix.slice(start, i));
        start = i + 1;
      }
    }
    parts.push(mix.slice(start));
    if (parts[0].trim() !== "in srgb") {
      bad.push(`color-mix(${mix}): interpolate in srgb`);
    }
    for (const operand of parts.slice(1)) {
      const colour = operand.replace(/\s+[\d.]+%\s*$/, "").replace(/^\s*[\d.]+%\s+/, "").trim();
      if (!/^var\(--[\w-]+\)$/.test(colour) && colour !== "transparent" && colour !== "currentColor") {
        bad.push(`color-mix(${mix}): operand "${colour}"`);
      }
    }
  }
  return bad;
}

type Declarations = Map<string, string>;

const byName = (a: string, b: string) => a.localeCompare(b);

/** `selector -> { --name: value }` for each top-level rule of the token file. */
function blocks(css: string): Map<string, Declarations> {
  const found = new Map<string, Declarations>();
  for (const rule of stripComments(css).matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
    const selector = rule[1].trim().replace(/\s+/g, " ");
    const declarations: Declarations = found.get(selector) ?? new Map();
    for (const declaration of rule[2].matchAll(/(--[^\s:]+)\s*:\s*([^;]+);/g)) {
      declarations.set(declaration[1], declaration[2].trim().replace(/\s+/g, " "));
    }
    found.set(selector, declarations);
  }
  return found;
}

const css = readFileSync(tokensCss, "utf8");
const parsed = blocks(css);
const scales = parsed.get(":root") ?? new Map();
const dark = parsed.get(':root, [data-theme="dark"]') ?? new Map();
const light = parsed.get('[data-theme="light"]') ?? new Map();
const declared = new Set([...scales.keys(), ...dark.keys(), ...light.keys()]);

interface Pin {
  source: { commit: string };
  dark: Record<string, string>;
  light: Record<string, string>;
  scale: {
    spacingPx: number[];
    fontSizeRem: Record<string, string>;
    tokens: Record<string, string>;
  };
}
const pin = JSON.parse(readFileSync(path.join(src, "tokens.pin.json"), "utf8")) as Pin;

// ---- the checks ---------------------------------------------------------------------------

describe("token file shape", () => {
  it("has exactly the scale, dark and light blocks", () => {
    expect([...parsed.keys()]).toEqual([":root", ':root, [data-theme="dark"]', '[data-theme="light"]']);
  });

  it("declares the same names for dark and light", () => {
    expect([...dark.keys()].sort(byName)).toEqual([...light.keys()].sort(byName));
  });

  it("keeps the theme-invariant scales apart from the theme colours", () => {
    const themed = new Set([...dark.keys(), ...light.keys()]);
    expect([...scales.keys()].filter((name) => themed.has(name))).toEqual([]);
    expect([...themed].filter((name) => !/^--pi-(color|shadow)-/.test(name))).toEqual([]);
    expect([...scales.keys()].filter((name) => /^--pi-(color|shadow)-/.test(name))).toEqual([]);
  });

  it("only declares legal custom-property names (no dot in a spacing key)", () => {
    for (const name of declared) {
      expect(name, "a `.` makes the whole declaration invalid CSS").toMatch(/^--pi-[A-Za-z0-9]+(?:-[A-Za-z0-9]+)*$/);
    }
  });
});

describe("token use", () => {
  const files = sources(src);

  it("scans the whole tree", () => {
    expect(files.length).toBeGreaterThan(8);
  });

  it("declares every var(--pi-…) any style or component reads", () => {
    for (const file of files) {
      const text = readFileSync(file, "utf8");
      expect(undeclared(text, declared), `${path.relative(src, file)}: not declared in tokens.css`).toEqual([]);
    }
  });

  it("writes no colour outside tokens.css", () => {
    for (const file of files) {
      expect(colourLiterals(readFileSync(file, "utf8")), `${path.relative(src, file)}: use a var(--pi-…) token`).toEqual([]);
    }
  });

  it("mixes colours in srgb from tokens only", () => {
    for (const file of files) {
      expect(badMixOperands(readFileSync(file, "utf8")), `${path.relative(src, file)}`).toEqual([]);
    }
  });

  it("keeps none of the template's unprefixed token names", () => {
    const legacy = /--(?:chrome|card|field|control|text|text-dim|accent|danger|separator|radius)\b(?!-)/;
    for (const file of [...files, tokensCss]) {
      const offending = stripComments(readFileSync(file, "utf8"))
        .split("\n")
        .filter((line) => legacy.test(line));
      expect(offending, `${path.relative(src, file)}`).toEqual([]);
    }
  });
});

describe("the checks catch what they are for", () => {
  it("fails on a typo'd token", () => {
    expect(undeclared("a { background: var(--pi-color-sufrace0); }", declared)).toEqual(["--pi-color-sufrace0"]);
    expect(undeclared("a { background: var(--pi-color-surface0); }", declared)).toEqual([]);
    expect(undeclared("a { padding: var(--pi-spacing-1.5); }", declared)).toEqual(["--pi-spacing-1.5"]);
  });

  it("fails on a colour literal", () => {
    for (const bad of ["color: #fff;", "color: #1a1a1e;", "color: rgb(0 0 0);", "color: rgba(0,0,0,.5);", "color: hsl(0 0% 0%);", "color: oklch(0.5 0.1 200);", "color: color(srgb 1 0 0);"]) {
      expect(colourLiterals(`a { ${bad} }`), `${bad}`).toHaveLength(1);
    }
    expect(colourLiterals("/* #fff */ a { color: var(--pi-color-foreground); }")).toEqual([]);
  });

  it("fails on a literal inside color-mix", () => {
    expect(badMixOperands("a { background: color-mix(in srgb, var(--pi-color-accent) 18%, transparent); }")).toEqual([]);
    expect(badMixOperands("a { background: color-mix(in srgb, var(--pi-color-accent) 12%, var(--pi-color-surface1)); }")).toEqual([]);
    expect(badMixOperands("a { background: color-mix(in srgb, red 18%, transparent); }")).toHaveLength(1);
    expect(badMixOperands("a { background: color-mix(in oklab, var(--pi-color-accent) 18%, transparent); }")).toHaveLength(1);
  });
});

describe("value pin (pi-studio commit recorded in tokens.pin.json)", () => {
  const norm = (value: string | undefined) => value?.toLowerCase();

  it("pins a real commit", () => {
    expect(pin.source.commit).toMatch(/^[0-9a-f]{40}$/);
  });

  it.each([
    ["dark", dark, pin.dark],
    ["light", light, pin.light],
  ] as const)("%s colours and shadows equal the pin, and nothing is unpinned", (_name, actual, expected) => {
    expect([...actual.keys()].sort(byName)).toEqual(Object.keys(expected).sort(byName));
    for (const [token, value] of Object.entries(expected)) {
      expect(norm(actual.get(token)), `${token}`).toBe(norm(value));
    }
  });

  it("resolves accentForeground to white in both themes (contrastForeground of each accent)", () => {
    expect(dark.get("--pi-color-accentForeground")).toBe("#ffffff");
    expect(light.get("--pi-color-accentForeground")).toBe("#ffffff");
  });

  it("scale tokens equal the pin", () => {
    const expected: Record<string, string> = { ...pin.scale.tokens };
    for (const px of pin.scale.spacingPx) {
      expected[`--pi-spacing-${px}`] = `${px}px`;
    }
    for (const [key, rem] of Object.entries(pin.scale.fontSizeRem)) {
      expected[`--pi-font-size-${key}`] = rem;
    }
    expect([...scales.keys()].sort(byName)).toEqual(Object.keys(expected).sort(byName));
    for (const [token, value] of Object.entries(expected)) {
      expect(scales.get(token), `${token}`).toBe(value);
    }
  });
});
