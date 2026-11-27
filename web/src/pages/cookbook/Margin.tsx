// The column beside the text on a wide window: what to know first, where to practise, and the sources, taken from the page
// itself (the full lists stay at the end). Blocks that span the page cross this column, so each card is placed in a stretch
// of the page that only text runs beside, and stays in sight while that stretch is on screen. Nothing here scrolls the page
// or hides content that is nowhere else.
import { useLayoutEffect, useRef } from "react";
import type { CbPage } from "../../api";
import { KIND_COLOR, sourceKind } from "../sd/SdRead";
import { Entry, GROUPS } from "./Lesson";

/** Space kept between a card and a block that spans the page, and between two cards. */
const GAP = 32;

/** An exercise that is exactly the page's task stays closed in Connections, so it is not offered here. */
const offered = (page: CbPage) => (page.connections ?? []).filter((c) => c.level !== "exact");

export const hasMargin = (page: CbPage) => offered(page).length > 0 || page.sources.length > 0;

/** The parts of the page's flow. A part may span the page; a component's inside is its own business. */
const FLOW = ":scope > :not(.cb-sec, .cb-lab, .cb-margin), :is(.cb-sec, .cb-lab) > :not(.sd-md), :is(.cb-sec, .cb-lab) > .sd-md > *";

/** The stretches of the article, top to bottom, that nothing in the flow reaches across into the column at `left`. */
function stretches(art: HTMLElement, left: number): [number, number][] {
    const box = art.getBoundingClientRect();
    const taken = [...art.querySelectorAll<HTMLElement>(FLOW)]
        .map((e) => e.getBoundingClientRect())
        .filter((r) => r.height > 0 && r.width > 2 && r.right > left - GAP)
        .map((r): [number, number] => [r.top - box.top, r.bottom - box.top])
        .sort((a, b) => a[0] - b[0]);
    const free: [number, number][] = [];
    let at = 0;
    for (const [top, bottom] of taken) {
        if (top - GAP > at) free.push([at, top - GAP]);
        at = Math.max(at, bottom + GAP);
    }
    if (box.height > at) free.push([at, box.height]);
    return free;
}

/** Puts the cards, in order, into the stretches: spread over them while there are enough, together where there are not. */
function place(column: HTMLElement) {
    const art = column.parentElement!;
    const slots = [...column.children] as HTMLElement[];
    if (!column.clientWidth) return;
    for (const s of slots) {
        // each card is measured at the column's width before it has its place
        s.style.cssText = "display:block;visibility:hidden";
        (s.firstElementChild as HTMLElement).style.cssText = "";
    }
    const high = slots.map((s) => (s.firstElementChild as HTMLElement).offsetHeight);
    const head = Number.parseFloat(getComputedStyle(art).getPropertyValue("--hdr-h")) || 52;
    const view = window.innerHeight - head - GAP;
    const free = stretches(art, column.getBoundingClientRect().left).filter(([a, b]) => b - a >= Math.min(...high));
    let next = 0;
    free.forEach(([from, to], k) => {
        const share = Math.ceil((slots.length - next) / (free.length - k));
        const here: number[] = [];
        let used = 0;
        while (next < slots.length && here.length < share) {
            const more = used + (here.length ? GAP : 0) + high[next]!;
            if (more > to - from || (here.length && more > view)) break;
            used = more;
            here.push(next++);
        }
        // each card's slot ends where the cards after it begin, so cards that stay in sight never meet
        let before = 0;
        for (const i of here) {
            const after = used - before - high[i]!;
            const slot = slots[i]!;
            slot.style.cssText = `display:block;top:${from + before}px;height:${to - from - before - after}px`;
            const card = slot.firstElementChild as HTMLElement;
            // a card that is taller than the window just scrolls with the page, so all of it can be reached
            if (before + high[i]! <= view) card.style.cssText = `position:sticky;top:${head + 12 + before}px`;
            before += high[i]! + GAP;
        }
    });
    // what found no room is left out here; it is all in the lists at the end of the page
    for (const s of slots.slice(next)) s.style.display = "none";
}

export function Margin({ page }: { page: CbPage }) {
    const column = useRef<HTMLElement>(null);
    useLayoutEffect(() => {
        const el = column.current;
        if (!el) return;
        let raf = 0;
        const again = () => {
            cancelAnimationFrame(raf);
            raf = requestAnimationFrame(() => place(el));
        };
        place(el);
        // the article's size changes when a fold opens, a viewer unfolds, fonts arrive or the window is resized
        const watch = new ResizeObserver(again);
        watch.observe(el.parentElement!);
        addEventListener("resize", again);
        void document.fonts.ready.then(again);
        return () => {
            watch.disconnect();
            removeEventListener("resize", again);
            cancelAnimationFrame(raf);
        };
    }, [page]);
    const list = offered(page);
    return (
        <aside className="cb-margin" ref={column} aria-label="Beside the text">
            {GROUPS.map(([kind, title]) => {
                const mine = list.filter((c) => c.kind === kind);
                if (!mine.length) return null;
                return (
                    <div className="cb-m-slot" key={kind}>
                        <div className="cb-m-card">
                            <h3>{title}</h3>
                            <ul>{mine.map((c, i) => <Entry key={i} c={c} />)}</ul>
                        </div>
                    </div>
                );
            })}
            {page.sources.length > 0 && (
                <div className="cb-m-slot">
                    <div className="cb-m-card">
                        <h3>Sources</h3>
                        <ul>
                            {page.sources.map((s) => (
                                <li key={s.url}>
                                    <em style={{ color: KIND_COLOR[sourceKind(s)] }}>{sourceKind(s)}</em>
                                    <a href={s.url} target="_blank" rel="noreferrer">{s.title}</a>
                                </li>
                            ))}
                        </ul>
                    </div>
                </div>
            )}
        </aside>
    );
}
