// Terminal text as the learner's tools print it: SGR colour and weight are kept as styles, every other escape sequence and
// control character is dropped. `parseAnsi` and `stripAnsi` share one tokenizer, so what is shown and what is searched,
// matched or copied are always the same characters. Plain TypeScript (no DOM), so Node can test it.

export interface AnsiStyle {
  /** A theme colour, always one of the `var(--ansi-…)` tokens: nothing from the input reaches CSS. */
  color?: string;
  bold?: boolean;
  dim?: boolean;
  italic?: boolean;
  underline?: boolean;
}

export interface Segment {
  text: string;
  style: AnsiStyle;
}

const NAMES = ["black", "red", "green", "yellow", "blue", "magenta", "cyan", "white"] as const;

/** The 16 terminal colours as theme tokens (defined in styles/app.css). */
const token = (n: number) => `var(--ansi-${n >= 8 ? "bright-" : ""}${NAMES[n % 8]})`;

/** The nearest theme colour to an RGB value: greys by lightness, everything else by hue. */
function nearest(r: number, g: number, b: number): string {
  const max = Math.max(r, g, b);
  const spread = max - Math.min(r, g, b);
  if (spread < 40) return token(max < 170 ? 8 : 7);
  const sixth = max === r ? (g - b) / spread : max === g ? (b - r) / spread + 2 : (r - g) / spread + 4;
  const hue = (sixth * 60 + 360) % 360;
  return token(hue < 20 || hue >= 330 ? 1 : hue < 70 ? 3 : hue < 165 ? 2 : hue < 200 ? 6 : hue < 255 ? 4 : 5);
}

/** A colour of the 256-colour table (`38;5;n`). */
function indexed(n: number): string | undefined {
  if (!Number.isInteger(n) || n < 0 || n > 255) return undefined;
  if (n < 16) return token(n);
  if (n >= 232) return token(n < 247 ? 8 : 7);
  const level = (v: number) => (v === 0 ? 0 : 55 + v * 40);
  const i = n - 16;
  return nearest(level(Math.floor(i / 36)), level(Math.floor(i / 6) % 6), level(i % 6));
}

/** Applies one SGR parameter list. Backgrounds are read (so their numbers aren't taken for colours) and not drawn. */
function sgr(style: AnsiStyle, params: string): AnsiStyle {
  const p = params.split(/[;:]/).map(Number);
  let s = style;
  for (let i = 0; i < p.length; i++) {
    const c = p[i]!;
    if (c === 0) s = {};
    else if (c === 1) s = { ...s, bold: true };
    else if (c === 2) s = { ...s, dim: true };
    else if (c === 3) s = { ...s, italic: true };
    else if (c === 4) s = { ...s, underline: true };
    else if (c === 22) s = { ...s, bold: false, dim: false };
    else if (c === 23) s = { ...s, italic: false };
    else if (c === 24) s = { ...s, underline: false };
    else if (c === 39) s = { ...s, color: undefined };
    else if (c >= 30 && c <= 37) s = { ...s, color: token(c - 30) };
    else if (c >= 90 && c <= 97) s = { ...s, color: token(c - 90 + 8) };
    else if (c === 38 || c === 48 || c === 58) {
      let color: string | undefined;
      if (p[i + 1] === 5) {
        color = indexed(p[i + 2] ?? -1);
        i += 2;
      } else if (p[i + 1] === 2) {
        const rgb = [p[i + 2], p[i + 3], p[i + 4]].map((v) => v ?? 0);
        color = nearest(rgb[0]!, rgb[1]!, rgb[2]!);
        i += 4;
      }
      if (c === 38 && color) s = { ...s, color };
    }
  }
  return s;
}

// In order: CSI (its final byte optional, so a sequence cut short by truncation goes too), OSC and the other string
// sequences (links, window titles: up to their terminator, or the end of the line), any other escape, a carriage return,
// and the remaining control characters (all but tab and newline).
const TOKEN =
  /(?:\x1b\[|\x9b)([0-?]*)[ -/]*([@-~])?|\x1b[\]PX^_][^\x07\x1b\n]*(?:\x07|\x1b\\)?|\x1b[ -/]*[0-~]?|(\r)|[\x00-\x08\x0b\x0c\x0e-\x1f\x7f-\x9f]/g;

/** Terminal text as lines of styled runs. A style carries across lines until it is reset, as in a terminal. */
export function ansiLines(text: string): Segment[][] {
  const lines: Segment[][] = [[]];
  let style: AnsiStyle = {};
  // A lone carriage return: the next text on this line writes over it (progress bars).
  let overwrite = false;
  const write = (chunk: string) => {
    chunk.split("\n").forEach((part, i) => {
      if (i > 0) {
        lines.push([]);
        overwrite = false;
      }
      if (!part) return;
      const line = lines[lines.length - 1]!;
      if (overwrite) {
        line.length = 0;
        overwrite = false;
      }
      const last = line[line.length - 1];
      if (last && last.style === style) last.text += part;
      else line.push({ text: part, style });
    });
  };
  let from = 0;
  for (const m of text.matchAll(TOKEN)) {
    write(text.slice(from, m.index));
    from = m.index + m[0].length;
    if (m[2] === "m" && /^[0-9;:]*$/.test(m[1]!)) style = sgr(style, m[1]!);
    else if (m[3]) overwrite = true;
  }
  write(text.slice(from));
  return lines;
}

/** The characters of one parsed line. */
export const lineText = (line: Segment[]) => line.map((s) => s.text).join("");

/** `text` as plain characters: what anything that parses, compares or copies output must read. */
export const stripAnsi = (text: string) => ansiLines(text).map(lineText).join("\n");

/** The inline CSS of a run, or null when it is drawn like the text around it. */
export function ansiCss({ color, bold, dim, italic, underline }: AnsiStyle): Record<string, string> | null {
  const css: Record<string, string> = {};
  // Bold with no colour of its own is the terminal's bright text; dim is its quiet text.
  const ink = color ?? (bold ? "var(--fg)" : dim ? "var(--dim)" : undefined);
  if (ink) css.color = ink;
  if (bold) css.fontWeight = "600";
  if (italic) css.fontStyle = "italic";
  if (underline) css.textDecoration = "underline";
  return Object.keys(css).length ? css : null;
}
