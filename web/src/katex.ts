// KaTeX and its stylesheet, loaded together the first time a page shows math (most pages have none).
import katex from "katex";
import "katex/dist/katex.min.css";

/** Math as HTML. `\htmlClass` is allowed (and nothing else that needs trust) so a page can colour a variable with a theme class. */
export function renderTex(tex: string, display: boolean): string {
    return katex.renderToString(tex, { displayMode: display, throwOnError: false, output: "htmlAndMathml", trust: (ctx) => ctx.command === "\\htmlClass", strict: "ignore" });
}
