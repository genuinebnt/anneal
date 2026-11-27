// Code that is wider than its box scrolls sideways with no bar. This marks such a box while there is more to its right, and
// the stylesheet fades that edge. The compare block marks its own (Lesson.tsx), since it also names them for the keyboard.
import { useEffect, type RefObject } from "react";

const mark = (pre: HTMLElement) => pre.toggleAttribute("data-more", pre.clientWidth > 0 && pre.scrollLeft + pre.clientWidth < pre.scrollWidth - 1);
const OWN = ".cb-cmp pre";

export function useCodeFades(article: RefObject<HTMLElement | null>) {
    useEffect(() => {
        const art = article.current;
        if (!art) return;
        let raf = 0;
        const all = () => {
            raf = 0;
            for (const pre of art.querySelectorAll("pre")) if (!pre.matches(OWN)) mark(pre);
        };
        const soon = () => {
            if (!raf) raf = requestAnimationFrame(all);
        };
        const scrolled = (e: Event) => {
            const el = e.target as HTMLElement;
            if (el.tagName === "PRE" && !el.matches(OWN)) mark(el);
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
