import { useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { createPortal } from "react-dom";
import { Link, useRouterState } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { api, type DsaProblem, type PatternExtra, type PatternLessons, type PatternListed, type PatternTechnique, type TechniqueLesson } from "../api";
import { LIST_LABEL, Mark, PRIORITY_LABEL, PyCode } from "../components/dsaBits";
import { PatternShell } from "../components/PatternShell";
import { useDialog } from "../components/useDialog";
import { DIFF } from "../dsa";
import { useShowIdeas } from "../spoilers";

/** A pattern's Learn page (docs/DSA_LEARN_PAGE_SPEC.md). Each technique is a card: its implementation on the left (when to use it, the
 *  Python template, the traps), the LeetCode problems that practise it on the right, whether or not they are in a NeetCode list.
 *  Techniques that have no problem are only named, at the bottom. The cards fold, and a rail beside them lists every group and technique,
 *  so a long page is easy to move around in. */
export function PatternPage({ code }: { code: string }) {
  const overview = useQuery({ queryKey: ["dsa"], queryFn: api.dsa });
  const lessons = useQuery({ queryKey: ["patterns", code], queryFn: () => api.patternLessons(code) });
  const today = overview.data?.today ?? "";
  const d = lessons.data;
  return (
    <PatternShell code={code} tab="learn">
      {lessons.isError && <p className="notice bad">Couldn't load the lessons: {(lessons.error as Error).message}</p>}
      {d && (
        <>
          <Learn d={d} today={today} />
          <OtherTechniques items={d.listed ?? []} />
        </>
      )}
    </PatternShell>
  );
}

const anchor = (id: string) => `t-${id.split(":")[1] ?? id}`;

/** One card of the page, a technique of the lists or an extra, with what the rail and the folded card need to show. */
interface Item {
  id: string;
  name: string;
  lesson: TechniqueLesson | null;
  problems: DsaProblem[];
  solved: number;
  lists: number;
  versions: number;
}

const itemOf = (t: PatternTechnique): Item => ({ id: t.id, name: t.name, lesson: t.lesson ?? null, problems: t.problems, solved: t.problems.filter((p) => p.state.solved && p.list_tag).length, lists: t.problems.filter((p) => p.list_tag).length, versions: t.lesson?.variant?.length ?? 0 });
const extraOf = (e: PatternExtra): Item => ({ id: e.id, name: e.name, lesson: e.lesson, problems: e.examples, solved: e.examples.filter((p) => p.state.solved && p.list_tag).length, lists: e.examples.filter((p) => p.list_tag).length, versions: e.lesson.variant?.length ?? 0 });

/** The page's sections: a pattern without groups is one section; otherwise a technique no group claims comes first, under "More", then the groups in order. */
function sectionsOf(d: PatternLessons): { name: string; items: Item[] }[] {
  if (!d.groups) return [{ name: "Techniques", items: d.techniques.map(itemOf) }];
  const groups = d.groups;
  const out: { name: string; items: Item[] }[] = [];
  const ungrouped = d.techniques.filter((t) => !t.lesson?.group || !groups.includes(t.lesson.group));
  if (ungrouped.length) out.push({ name: "More", items: ungrouped.map(itemOf) });
  for (const g of groups) {
    const items = [...d.techniques.filter((t) => t.lesson?.group === g).map(itemOf), ...d.extras.filter((e) => e.group === g).map(extraOf)];
    if (items.length) out.push({ name: g, items });
  }
  return out;
}

const status = (i: Item) => (i.lists > 0 && i.solved === i.lists ? "s" : i.solved > 0 ? "p" : "");
const ratio = (items: Item[]) => {
  const lists = items.reduce((n, i) => n + i.lists, 0);
  return lists ? Math.round((items.reduce((n, i) => n + i.solved, 0) / lists) * 100) : 0;
};

function Learn({ d, today }: { d: PatternLessons; today: string }) {
  const sections = useMemo(() => sectionsOf(d), [d]);
  const items = useMemo(() => sections.flatMap((s) => s.items), [sections]);
  const hash = useRouterState({ select: (s) => s.location.hash });
  const [open, setOpen] = useState<Set<string>>(() => new Set(items[0] ? [items[0].id] : []));
  const [cur, setCur] = useState(0);
  const [query, setQuery] = useState("");
  const [sheet, setSheet] = useState(false);

  // a link to a card (#t-...) opens it and brings it into view
  useEffect(() => {
    const it = items.find((i) => anchor(i.id) === hash);
    if (!it) return;
    setOpen((o) => new Set(o).add(it.id));
    const t = window.setTimeout(() => document.getElementById(hash)?.scrollIntoView({ block: "start" }), 60);
    return () => window.clearTimeout(t);
  }, [hash, items]);

  // the rail follows the section being read
  useEffect(() => {
    const seen = new IntersectionObserver((es) => es.forEach((e) => e.isIntersecting && setCur(Number((e.target as HTMLElement).dataset.g))), { rootMargin: "-130px 0px -70% 0px" });
    document.querySelectorAll("section[data-g]").forEach((el) => seen.observe(el));
    return () => seen.disconnect();
  }, [sections]);

  const toggle = (id: string) => setOpen((o) => { const n = new Set(o); if (!n.delete(id)) n.add(id); return n; });
  const pick = (i: Item) => {
    setOpen((o) => new Set(o).add(i.id));
    setSheet(false);
    window.setTimeout(() => document.getElementById(anchor(i.id))?.scrollIntoView({ block: "start", behavior: "smooth" }), 60);
  };
  const toGroup = (g: number) => {
    setSheet(false);
    document.getElementById(`lr-g${g}`)?.scrollIntoView({ block: "start", behavior: "smooth" });
  };
  const rail = <Rail sections={sections} cur={cur} query={query} setQuery={setQuery} onPick={pick} onGroup={toGroup} openAll={() => setOpen(new Set(items.map((i) => i.id)))} foldAll={() => setOpen(new Set())} />;

  return (
    <div className="lr-wrap">
      <aside className="lr-rail" aria-label="Techniques">{rail}</aside>
      <div className="lr-main">
        <div className="lr-jumpbar">
          <button onClick={() => setSheet(true)}>☰ Jump to…</button>
          <span>{sections[cur]?.name}</span>
        </div>
        {sections.map((s, g) => (
          <section key={s.name} className="lr-sec" id={`lr-g${g}`} data-g={g} aria-label={s.name}>
            <div className="lr-gh"><h2>{s.name}</h2><small>{s.items.length} technique{s.items.length === 1 ? "" : "s"}</small></div>
            {s.items.map((i) => <Card key={i.id} item={i} open={open.has(i.id)} toggle={() => toggle(i.id)} today={today} />)}
          </section>
        ))}
      </div>
      {sheet && <Sheet onClose={() => setSheet(false)}>{rail}</Sheet>}
    </div>
  );
}

/** The list beside the cards: a search box, every group with how far you are in it, and the techniques of the group being read. */
function Rail({ sections, cur, query, setQuery, onPick, onGroup, openAll, foldAll }: { sections: { name: string; items: Item[] }[]; cur: number; query: string; setQuery: (q: string) => void; onPick: (i: Item) => void; onGroup: (g: number) => void; openAll: () => void; foldAll: () => void }) {
  const [shown, setShown] = useState<Set<number>>(() => new Set([0]));
  const q = query.trim().toLowerCase();
  const total = sections.reduce((n, s) => n + s.items.length, 0);
  return (
    <>
      <div className="lr-find"><input type="search" value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Find a technique…" aria-label="Find a technique" autoComplete="off" /></div>
      <div className="lr-tools"><span>{total} TECHNIQUES</span><span><button onClick={openAll}>open all</button> · <button onClick={foldAll}>fold all</button></span></div>
      {sections.map((s, g) => {
        const items = q ? s.items.filter((i) => i.name.toLowerCase().includes(q)) : s.items;
        if (q && items.length === 0) return null;
        const isOpen = q ? true : shown.has(g) || g === cur;
        return (
          <div key={s.name} className={`lr-grp${isOpen ? " open" : ""}${g === cur ? " cur" : ""}`}>
            <button className="lr-gb" aria-expanded={isOpen} onClick={() => { setShown((x) => { const n = new Set(x); if (!n.delete(g)) n.add(g); return n; }); onGroup(g); }}>
              <i className="lr-ring" style={{ "--p": ratio(s.items) } as CSSProperties} aria-hidden="true" />
              <span>{s.name}</span><em>{s.items.length}</em>
              <svg viewBox="0 0 8 12" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M1.5 1.5L6 6l-4.5 4.5" /></svg>
            </button>
            {isOpen && (
              <div className="lr-tl">
                {items.map((i) => <button key={i.id} onClick={() => onPick(i)}><b className={status(i)} aria-hidden="true" />{i.name}</button>)}
              </div>
            )}
          </div>
        );
      })}
    </>
  );
}

/** The rail as a sheet from the bottom, for a phone. */
function Sheet({ children, onClose }: { children: React.ReactNode; onClose: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  useDialog(ref, true, onClose);
  return createPortal(
    <div className="lr-scrim" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="lr-sheet" ref={ref} role="dialog" aria-label="Jump to a technique">{children}</div>
    </div>,
    document.body,
  );
}

/** A technique as a folded card: its name, how you are doing, and how many problems; open, it shows the implementation and the problems. */
function Card({ item, open, toggle, today }: { item: Item; open: boolean; toggle: () => void; today: string }) {
  const id = anchor(item.id);
  const [opened, setOpened] = useState(open);
  useEffect(() => { if (open) setOpened(true); }, [open]);
  const first = item.lesson?.signals[0];
  return (
    <article className={`lr-card${open ? " open" : ""}`} id={id}>
      <button className="lr-head" aria-expanded={open} aria-controls={`${id}-body`} onClick={toggle}>
        <i className={`lr-dot ${status(item)}`} aria-hidden="true" />
        <span className="lr-name"><b>{item.name}</b>{first && <small>{first}</small>}</span>
        {item.versions > 0 && <span className="lr-ver">{item.versions + 1} VERSIONS</span>}
        <span className="lr-count">{item.lists > 0 && <>{item.solved}/{item.lists} · </>}{item.problems.length} problem{item.problems.length === 1 ? "" : "s"}</span>
        <svg className="lr-chev" viewBox="0 0 12 8" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M1.5 1.5L6 6.5l4.5-5" /></svg>
      </button>
      <div className="lr-body" id={`${id}-body`}>
        <div>
          {opened && (
            <div className="lr-split">
              <div className="lr-left">
                {item.lists > 0 && <span className="lr-prog">{item.solved} / {item.lists} in the lists<i><b style={{ width: `${(item.solved / item.lists) * 100}%` }} /></i></span>}
                {item.lesson ? <Implementation lesson={item.lesson} /> : <p className="l-soon">The implementation isn't written yet.</p>}
              </div>
              <Problems problems={item.problems} today={today} />
            </div>
          )}
        </div>
      </div>
    </article>
  );
}

/** The problems of a card, in a box as tall as the implementation beside it: a long list scrolls inside it. */
function Problems({ problems, today }: { problems: DsaProblem[]; today: string }) {
  const box = useRef<HTMLDivElement>(null);
  const [more, setMore] = useState(false);
  useEffect(() => {
    const el = box.current;
    if (!el) return;
    const measure = () => setMore(el.scrollHeight - el.clientHeight - el.scrollTop > 6);
    measure();
    el.addEventListener("scroll", measure, { passive: true });
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => { el.removeEventListener("scroll", measure); ro.disconnect(); };
  }, [problems]);
  const inLists = problems.filter((p) => p.list_tag);
  const others = problems.filter((p) => !p.list_tag);
  const solved = inLists.filter((p) => p.state.solved).length;
  return (
    <div className="lr-right">
      <div className="lr-rin" ref={box} tabIndex={0} aria-label={`Problems, ${problems.length}`}>
        <div className="lr-rh"><span className="lr-lab">PROBLEMS · {problems.length}</span>{inLists.length > 0 && <small>{solved} of {inLists.length} solved in the lists</small>}</div>
        {inLists.length > 0 && <div className="lr-sub">IN THE LISTS <em>{inLists.length}</em></div>}
        {inLists.map((p) => <ProblemRow key={p.id} p={p} today={today} />)}
        {others.length > 0 && <div className="lr-sub">{inLists.length > 0 ? "ALSO PRACTISES THIS" : "PRACTICE"} <em>{others.length}</em></div>}
        {others.map((p) => <ProblemRow key={p.id} p={p} today={today} />)}
      </div>
      <div className={`lr-fade${more ? " on" : ""}`} aria-hidden="true" />
      <span className={`lr-more${more ? " on" : ""}`} aria-hidden="true">↓ more</span>
    </div>
  );
}

/** A problem as two quiet lines: the mark, the title (it opens the site's problem page), difficulty and priority as the only loud tags;
 *  then the list, recent and solution words, the companies and the topics (every tag of the spec's section 3). The idea is one click away. */
function ProblemRow({ p, today }: { p: DsaProblem; today: string }) {
  const [always] = useShowIdeas();
  const [open, setOpen] = useState(false);
  const shown = always || open;
  return (
    <div className={`lr-p pri-${p.priority}${p.list_tag ? "" : " out"}`}>
      <Mark p={p} today={today} />
      <Link to="/d/$slug" params={{ slug: p.slug }} className="lr-pt"><small>#{p.number}</small><b>{p.title}</b></Link>
      <span className="lr-pills">
        <span className="lr-pill lv" style={{ color: DIFF[p.difficulty][1] }}>{DIFF[p.difficulty][0].toUpperCase()}</span>
        <span className={`lr-pill pri-${p.priority}`} title={p.priority === "must" ? "Must solve" : p.priority === "strong" ? "Strong: asked often" : p.priority === "warmup" ? "A warm-up" : "Practice"}>{PRIORITY_LABEL[p.priority]}</span>
      </span>
      <div className="lr-pm">
        {(p.list_tag || p.recent || p.has_page || p.premium) && (
          <span className="lr-q">
            {p.list_tag && <span className="lst" title="The narrowest NeetCode list this problem is in">{LIST_LABEL[p.list_tag]}</span>}
            {p.recent && <span className="rec" title="Asked in the last six months">RECENT</span>}
            {p.has_page && <span className="sol" title="This site has a written solution for it">SOLUTION</span>}
            {p.premium && <span className="prem">PREM</span>}
          </span>
        )}
        {p.companies.length > 0 && (
          <span className="lr-co">
            {p.companies.slice(0, 3).map((c, k) => <span key={c.name} className={c.recent ? "hot" : ""} title={c.recent ? `${c.name}: asked in the last six months` : c.name}>{c.name}{k < Math.min(2, p.companies.length - 1) ? "," : ""}</span>)}
            {p.companies.length > 3 && <span className="more" title={p.companies.slice(3).map((c) => c.name).join(", ")}>+{p.companies.length - 3}</span>}
          </span>
        )}
        {p.tags.length > 0 && <span className="lr-tp">{p.tags.slice(0, 3).join(" · ")}</span>}
        {p.insight && !shown && <button className="lr-idea" onClick={() => setOpen(true)} aria-label="Show the key idea">Idea</button>}
      </div>
      {p.insight && shown && <p className="lr-ins">{p.insight}</p>}
    </div>
  );
}

/** Techniques of this topic that have no problem attached (or no implementation yet): named, nothing more. */
function OtherTechniques({ items }: { items: PatternListed[] }) {
  const [open, setOpen] = useState(false);
  if (items.length === 0) return null;
  return (
    <section className="l-other" aria-label="Other techniques">
      <button className="l-ltoggle" aria-expanded={open} onClick={() => setOpen(!open)}>
        <span aria-hidden="true">{open ? "▾" : "▸"}</span>
        Other techniques of this topic ({items.length})
        <small> · no LeetCode problem is attached to them here</small>
      </button>
      {open && (
        <ul className="l-onames">
          {items.map((l) => (
            <li key={`${l.group}:${l.name}`}>{l.name}</li>
          ))}
        </ul>
      )}
    </section>
  );
}

/** The template, with a tab for each variant (recursive and iterative DFS). */
function Templates({ lesson }: { lesson: TechniqueLesson }) {
  const variants = lesson.variant ?? [];
  const [i, setI] = useState(0);
  const first = { name: lesson.template_name ?? "Template", note: "", template: lesson.template };
  const tabs = variants.length ? [first, ...variants] : [];
  const cur = tabs[i] ?? first;
  return (
    <div className="lr-codebox">
      <div className="lr-cbar">
        <span className="lr-lang">PYTHON</span>
        {tabs.length > 0 && (
          <div className="lr-tabs" role="tablist" aria-label="Versions of the template">
            {tabs.map((t, k) => <button key={t.name} role="tab" aria-selected={k === i} onClick={() => setI(k)}>{t.name}</button>)}
          </div>
        )}
      </div>
      {cur.note && <p className="lr-tabnote">{cur.note}</p>}
      <PyCode code={cur.template.trim()} lineNumbers />
    </div>
  );
}

/** The left of a card: when to use it, the implementation, the traps. */
function Implementation({ lesson }: { lesson: TechniqueLesson }) {
  return (
    <>
      <div className="lr-use">
        <span className="lr-lab">USE IT WHEN</span>
        {lesson.signals.map((s) => <div key={s}>{s}</div>)}
      </div>
      <Templates lesson={lesson} />
      <div className="lr-traps">
        <span className="lr-lab">TRAPS</span>
        {lesson.pitfalls.map((p) => <div key={p} className="lr-trap">{p}</div>)}
      </div>
    </>
  );
}
