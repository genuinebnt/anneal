// A diagnostic card that stays put. The hover popup closes as soon as the pointer leaves it, which is no good for reading
// rustc's full output, so opening that output moves the card here. It closes on ×, Escape, a click elsewhere or an edit.
import { StateEffect, StateField } from "@codemirror/state";
import { EditorView, ViewPlugin, closeHoverTooltips, showTooltip, type Tooltip } from "@codemirror/view";
import type { Diagnostic } from "../api";
import { diagCard } from "./diagcard";

const setPinned = StateEffect.define<Tooltip | null>();

const pinnedField = StateField.define<Tooltip | null>({
  create: () => null,
  update(value, tr) {
    for (const e of tr.effects) if (e.is(setPinned)) return e.value;
    return tr.docChanged ? null : value;
  },
  provide: (f) => showTooltip.from(f),
});

const dismiss = ViewPlugin.fromClass(
  class {
    private readonly outside: (e: MouseEvent) => void;
    private readonly escape: (e: KeyboardEvent) => void;
    constructor(view: EditorView) {
      const close = () => view.state.field(pinnedField) && view.dispatch({ effects: setPinned.of(null) });
      this.outside = (e) => {
        if (!(e.target instanceof Element && e.target.closest(".cm-diag-pinned"))) close();
      };
      this.escape = (e) => {
        if (e.key === "Escape") close();
      };
      document.addEventListener("mousedown", this.outside, true);
      document.addEventListener("keydown", this.escape, true);
    }
    destroy() {
      document.removeEventListener("mousedown", this.outside, true);
      document.removeEventListener("keydown", this.escape, true);
    }
  },
);

export const pinnedDiagnostic = [pinnedField, dismiss];

/** Replaces the hover popup with a card for `d`, its full output open, anchored at `pos`. */
export function pinDiagnostic(view: EditorView, pos: number, d: Diagnostic) {
  const tooltip: Tooltip = {
    pos: Math.min(pos, view.state.doc.length),
    above: false,
    create: (v) => {
      const dom = document.createElement("div");
      dom.className = "cm-diag-pinned";
      const card = diagCard(d, v.state.doc.toString().split("\n"), { open: true });
      const close = document.createElement("button");
      close.className = "lens-x";
      close.title = "Close (Esc)";
      close.setAttribute("aria-label", "Close this error");
      close.textContent = "×";
      close.addEventListener("click", () => v.dispatch({ effects: setPinned.of(null) }));
      card.append(close);
      dom.append(card);
      return { dom };
    },
  };
  view.dispatch({ effects: [setPinned.of(tooltip), closeHoverTooltips] });
}
