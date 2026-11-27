import { Fragment } from "react";
import { ansiCss, ansiLines, type Segment } from "./ansiText";

export { ansiLines, lineText, stripAnsi } from "./ansiText";

/** One parsed line as styled spans. Text goes in as text nodes, so output can't inject markup. */
export function AnsiLine({ line }: { line: Segment[] }) {
  return (
    <>
      {line.map((s, i) => {
        const css = ansiCss(s.style);
        return css ? (
          <span key={i} style={css}>
            {s.text}
          </span>
        ) : (
          <Fragment key={i}>{s.text}</Fragment>
        );
      })}
    </>
  );
}

/** Renders terminal output, coloured from the theme. */
export function Ansi({ text }: { text: string }) {
  return (
    <>
      {ansiLines(text).map((line, i) => (
        <Fragment key={i}>
          {i > 0 && "\n"}
          <AnsiLine line={line} />
        </Fragment>
      ))}
    </>
  );
}

/** The same rendering into plain DOM, for CodeMirror widgets and tooltips that aren't React. */
export function appendAnsi(parent: HTMLElement, text: string) {
  ansiLines(text).forEach((line, i) => {
    if (i > 0) parent.append("\n");
    for (const s of line) {
      const css = ansiCss(s.style);
      if (!css) {
        parent.append(s.text);
        continue;
      }
      const span = document.createElement("span");
      Object.assign(span.style, css);
      span.textContent = s.text;
      parent.append(span);
    }
  });
}
