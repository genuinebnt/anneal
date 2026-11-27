// Code, a table or a drawing that is wider than its box scrolls sideways with no bar. This marks such a box while there is
// more to its right, and the stylesheet fades that edge. A table or drawing that scrolls can also take the keyboard's focus,
// so the arrow keys reach the rest. The compare block marks its own (Lesson.tsx), since it also names them for the keyboard.
import { useEffect, type RefObject } from "react";

const mark = (box: HTMLElement) => {
    box.toggleAttribute("data-more", box.clientWidth > 0 && box.scrollLeft + box.clientWidth < box.scrollWidth - 1);
    if (!box.matches(BLOCKS)) return;
    if (box.clientWidth > 0 && box.scrollWidth > box.clientWidth + 1) box.tabIndex = 0;
    else box.removeAttribute("tabindex");
};
const OWN = ".cb-cmp pre";
/** The boxes of tables and drawings (cookbook-guide.css). */
const BLOCKS = ".sd-tbl, .cb-g-tt, .cb-g-pipe-wrap, .cb-g-fig-scroll";
const BOXES = `pre, ${BLOCKS}`;

export function useCodeFades(article: RefObject<HTMLElement | null>) {
    useEffect(() => {
        const art = article.current;
        if (!art) return;
        let raf = 0;
        const all = () => {
            raf = 0;
            for (const box of art.querySelectorAll<HTMLElement>(BOXES)) if (!box.matches(OWN)) mark(box);
        };
        const soon = () => {
            if (!raf) raf = requestAnimationFrame(all);
        };
        const scrolled = (e: Event) => {
            const el = e.target as HTMLElement;
            if (el.matches(BOXES) && !el.matches(OWN)) mark(el);
        };
        // new content, an opened fold, another file, a text size or "Wrap code" can each change what fits
        const content = new MutationObserver(soon);
        content.observe(art, { subtree: true, childList: true, attributes: true, attributeFilter: ["open", "class", "style"] });
        const page = art.closest("main");
        if (page) content.observe(page, { attributes: true, attributeFilter: ["class"] });
        const size = new ResizeObserver(soon);
        size.observe(art);
        art.addEventListener("scroll", scrolled, true);
        addEventListener("resize", soon);
        void document.fonts.ready.then(soon);
        soon();
        return () => {
            content.disconnect();
            size.disconnect();
            art.removeEventListener("scroll", scrolled, true);
            removeEventListener("resize", soon);
            cancelAnimationFrame(raf);
        };
    }, [article]);
}
