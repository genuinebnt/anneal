import { Fragment } from "react";
import { AnsiLine } from "./ansi";
import { ansiLines, lineText } from "./ansiText";

/** The line classes that stand in for colour on text that carries none (course runs reported before the CLI sent colour). */
const plainClass = (l: string) => (/^error(\[|:)/.test(l) ? "k-er" : /^\s*-->/.test(l) ? "k-pt2" : /^\s*(\d+\s*)?\|/.test(l) ? "k-dm2" : undefined);

/** What cargo printed for a course run: its own colours where it has them. */
export function CompilerText({ text }: { text: string }) {
  const coloured = text.includes("\x1b[");
  return (
    <>
      {ansiLines(text).map((line, i) => (
        <Fragment key={i}>
          <span className={coloured ? undefined : plainClass(lineText(line))}>
            <AnsiLine line={line} />
          </span>
          {"\n"}
        </Fragment>
      ))}
    </>
  );
}
