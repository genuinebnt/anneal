import { useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { Link, useRouterState } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { api, type DsaOverview, type DsaProblem, type PatternExtra, type PatternLessons, type PatternListed, type PatternTechnique, type TechniqueLesson } from "../api";
import { LIST_LABEL, Mark, PRIORITY_LABEL, PyCode } from "../components/dsaBits";
import { IdeaGlyph } from "../components/IdeaGlyph";
import { HowFound, ListCard } from "../components/layout";
import { PatternShell } from "../components/PatternShell";
import { DIFF, statusOf } from "../dsa";
import { getPref, setPref } from "../prefs";
import { useShowIdeas } from "../spoilers";

/** A pattern's Learn page (docs/DSA_LEARN_PAGE_SPEC.md). Each technique is a card: its implementation on the left (when to use it, the
 *  Python template, the traps), the LeetCode problems that practise it on the right, whether or not they are in a NeetCode list.
 *  Techniques that have no problem are only named, at the bottom. The cards fold, and a rail beside them lists every group and technique,
 *  so a long page is easy to move around in. */
export function PatternPage({ code }: { code: string }) {
  const overview = useQuery({ queryKey: ["dsa"], queryFn: api.dsa });
  const lessons = useQuery({ queryKey: ["patterns", code], queryFn: () => api.patternLessons(code) });
  const d = lessons.data;
  if (!d)
    return (
      <PatternShell code={code} tab="learn">
        {lessons.isError && <p className="notice bad">Couldn't load the lessons: {(lessons.error as Error).message}</p>}
      </PatternShell>
    );
  return <Learn key={d.code} code={code} d={d} o={overview.data} />;
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
  /** Problems of the card whose review is due today. */
  due: number;
}

const dueOf = (ps: DsaProblem[], today: string) => (today ? ps.filter((p) => statusOf(p, today) === "due").length : 0);
const itemOf = (t: PatternTechnique, today: string): Item => ({ id: t.id, name: t.name, lesson: t.lesson ?? null, problems: t.problems, solved: t.problems.filter((p) => p.state.solved && p.list_tag).length, lists: t.problems.filter((p) => p.list_tag).length, versions: t.lesson?.variant?.length ?? 0, due: dueOf(t.problems, today) });
const extraOf = (e: PatternExtra, today: string): Item => ({ id: e.id, name: e.name, lesson: e.lesson, problems: e.examples, solved: e.examples.filter((p) => p.state.solved && p.list_tag).length, lists: e.examples.filter((p) => p.list_tag).length, versions: e.lesson.variant?.length ?? 0, due: dueOf(e.examples, today) });

/** The page's sections: a pattern without groups is one section; otherwise a technique no group claims comes first, under "More", then the groups in order. */
function sectionsOf(d: PatternLessons, today: string): { name: string; items: Item[] }[] {
  const itemOf_ = (t: PatternTechnique) => itemOf(t, today);
  const extraOf_ = (e: PatternExtra) => extraOf(e, today);
  if (!d.groups) return [{ name: "Techniques", items: d.techniques.map(itemOf_) }];
  const groups = d.groups;
  const out: { name: string; items: Item[] }[] = [];
  const ungrouped = d.techniques.filter((t) => !t.lesson?.group || !groups.includes(t.lesson.group));
  if (ungrouped.length) out.push({ name: "More", items: ungrouped.map(itemOf_) });
  for (const g of groups) {
    const items = [...d.techniques.filter((t) => t.lesson?.group === g).map(itemOf_), ...d.extras.filter((e) => e.group === g).map(extraOf_)];
    if (items.length) out.push({ name: g, items });
  }
  return out;
}

const status = (i: Item) => (i.lists > 0 && i.solved === i.lists ? "s" : i.solved > 0 ? "p" : "");
const ratio = (items: Item[]) => {
  const lists = items.reduce((n, i) => n + i.lists, 0);
  return lists ? Math.round((items.reduce((n, i) => n + i.solved, 0) / lists) * 100) : 0;
};

function Learn({ code, d, o }: { code: string; d: PatternLessons; o: DsaOverview | undefined }) {
  const today = o?.today ?? "";
  const sections = useMemo(() => sectionsOf(d, today), [d, today]);
  const items = useMemo(() => sections.flatMap((s) => s.items), [sections]);
  const hash = useRouterState({ select: (s) => s.location.hash });
  const [open, setOpen] = useState<Set<string>>(() => new Set(items[0] ? [items[0].id] : []));
  const [cur, setCur] = useState(0);
  const [query, setQuery] = useState("");
  const [focus, setFocus] = useState(() => items[0]?.id ?? "");
  const [mini, setMini] = useState(() => getPref<string>("lr.rail", "full") === "mini");
  const find = useRef<HTMLInputElement>(null);
  const kb = useRef(-1);
  const setRail = (m: boolean) => { setMini(m); setPref("lr.rail", m ? "mini" : "full"); };

  // keys: / finds a technique, j and k move between the problems of the open cards, Enter opens the one marked, o folds or opens the card in view
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.target as HTMLElement | null)?.closest?.("input,textarea,select,[contenteditable]") || e.metaKey || e.ctrlKey || e.altKey) return;
      const rows = [...document.querySelectorAll<HTMLElement>(".lr-card.open .lr-p")];
      if (e.key === "/") {
        e.preventDefault();
        find.current?.focus();
      } else if ((e.key === "j" || e.key === "k") && rows.length) {
        kb.current = Math.max(0, Math.min(rows.length - 1, kb.current + (e.key === "j" ? 1 : -1)));
        rows.forEach((r, i) => r.classList.toggle("kb", i === kb.current));
        rows[kb.current]!.scrollIntoView({ block: "nearest", behavior: "smooth" });
      } else if (e.key === "Enter" && kb.current >= 0 && rows[kb.current]) {
        rows[kb.current]!.querySelector<HTMLAnchorElement>("a.lr-pt")?.click();
      } else if (e.key === "o") {
        const mid = innerHeight / 2;
        const cards = [...document.querySelectorAll<HTMLElement>(".lr-card")];
        const c = cards.sort((a, b) => Math.abs(a.getBoundingClientRect().top - mid) - Math.abs(b.getBoundingClientRect().top - mid))[0];
        c?.querySelector<HTMLButtonElement>(".lr-head")?.click();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // a link to a card (#t-...) opens it and brings it into view
  useEffect(() => {
    const it = items.find((i) => anchor(i.id) === hash);
    if (!it) return;
    setOpen((o) => new Set(o).add(it.id));
    setFocus(it.id);
    const t = window.setTimeout(() => document.getElementById(hash)?.scrollIntoView({ block: "start" }), 60);
    return () => window.clearTimeout(t);
  }, [hash, items]);

  // the rail follows the section being read
  useEffect(() => {
    const seen = new IntersectionObserver((es) => es.forEach((e) => e.isIntersecting && setCur(Number((e.target as HTMLElement).dataset.g))), { rootMargin: "-130px 0px -70% 0px" });
    document.querySelectorAll("section[data-g]").forEach((el) => seen.observe(el));
    return () => seen.disconnect();
  }, [sections]);

  const toggle = (id: string) => setOpen((o) => { const n = new Set(o); if (!n.delete(id)) { n.add(id); setFocus(id); } return n; });
  const pick = (i: Item) => {
    setOpen((o) => new Set(o).add(i.id));
    setFocus(i.id);
    window.setTimeout(() => document.getElementById(anchor(i.id))?.scrollIntoView({ block: "start", behavior: "smooth" }), 60);
  };
  const toGroup = (g: number) => {
    document.getElementById(`lr-g${g}`)?.scrollIntoView({ block: "start", behavior: "smooth" });
  };
  // a technique an Idea tab describes gets that row's small picture in its header
  const glyphs = useMemo(() => new Map((d.idea?.which ?? []).map((w) => [w.technique, w.glyph] as const)), [d]);
  const rail = <Rail pattern={d.pattern} sections={sections} cur={cur} open={open} query={query} setQuery={setQuery} onPick={pick} onGroup={toGroup} openAll={() => setOpen(new Set(items.map((i) => i.id)))} foldAll={() => setOpen(new Set())} mini={mini} setMini={setRail} find={find} />;

  const current = items.find((i) => i.id === focus) ?? items[0];

  return (
    <PatternShell
      code={code}
      tab="learn"
      mini={mini}
      left={<nav className="lr-rail" aria-label="Techniques">{rail}</nav>}
      right={current && o ? <About item={current} o={o} /> : undefined}
    >
      <div className="lr-main">
        {sections.map((s, g) => (
          <section key={s.name} className="lr-sec" id={`lr-g${g}`} data-g={g} aria-label={s.name}>
            <div className="lr-gh"><h2>{s.name}</h2><small>{s.items.length} technique{s.items.length === 1 ? "" : "s"}</small></div>
            {s.items.map((i) => <Card key={i.id} item={i} open={open.has(i.id)} toggle={() => toggle(i.id)} today={today} glyph={glyphs.get(i.id)} />)}
          </section>
        ))}
      </div>
      <OtherTechniques items={d.listed ?? []} />
    </PatternShell>
  );
}

/** The right panel: the techniques of every pattern nearest the one being read (by the LeetCode topics their problems share), and how they
 *  were found. Everything is counted from the catalog; nothing is guessed. The rail already lists this pattern's own techniques. */
function About({ item, o }: { item: Item; o: DsaOverview }) {
  const near = useMemo(() => neighbours(item, o), [item, o]);
  return (
    <>
      <section className="ly-pn lr-about">
        <h5 className="ly-pnh"><span>About this technique</span></h5>
        <h3 className="lr-about-t">{item.name}</h3>
        <span className="ly-lab">Nearest techniques, in any pattern</span>
        {near.list.length ? (
            <div className="ly-cards">
              {near.list.map((n, k) => (
                <ListCard
                  key={n.id}
                  i={k}
                  title={n.name}
                  note={`${n.shared.length} shared`}
                  w={near.top ? (n.shared.length / near.top) * 100 : 0}
                  color={n.color}
                  to={n.code ? { to: "/dsa/patterns/$code", params: { code: n.code }, hash: anchor(n.id) } : undefined}
                  chips={<><span className="ly-pat" style={{ "--c": n.color } as CSSProperties}>{n.pattern}</span><span className="lr-tags">{n.shared.slice(0, 3).map((t) => <span key={t}>{t}</span>)}</span></>}
                />
              ))}
            </div>
          ) : (
            <p>No other technique shares a topic with this one's problems.</p>
          )}
      </section>
      <HowFound
        text="Shared LeetCode topic tags between this technique's problems and every other technique's problems, ranked by how many they share; a rarer topic breaks a tie."
        rows={[["Compared", `${near.compared} techniques`], ["Ranked by", "shared topics"], ["Topics here", near.topics], ["Solved among them", `${near.solved} of ${near.total}`]]}
      />
    </>
  );
}

const PATTERN_COLORS = ["var(--acc)", "var(--grn)", "var(--vio)", "var(--warn)", "var(--fn)", "var(--bad)"];

/** Techniques whose problems carry the most LeetCode topics in common with this one's. */
function neighbours(item: Item, o: DsaOverview) {
  const byTech = new Map<string, DsaProblem[]>();
  for (const p of o.problems) {
    if (!p.technique) continue;
    const l = byTech.get(p.technique);
    if (l) l.push(p);
    else byTech.set(p.technique, [p]);
  }
  const tagsOf = (ps: DsaProblem[]) => new Set(ps.flatMap((p) => p.tags));
  const mine = tagsOf(item.problems.length ? item.problems : byTech.get(item.id) ?? []);
  // how many techniques use a topic: the fewer, the more a shared one says
  const uses = new Map<string, number>();
  for (const ps of byTech.values()) for (const t of tagsOf(ps)) uses.set(t, (uses.get(t) ?? 0) + 1);
  const names = new Map(o.techniques.map((t) => [t.id, t] as const));
  const codes = new Map(o.patterns.map((p, i) => [p.name, { code: p.code, color: PATTERN_COLORS[i % PATTERN_COLORS.length]! }] as const));
  const scored = [...byTech.entries()]
    .filter(([id]) => id !== item.id && names.has(id))
    .map(([id, ps]) => {
      const shared = [...tagsOf(ps)].filter((t) => mine.has(t)).sort((a, b) => (uses.get(a) ?? 0) - (uses.get(b) ?? 0));
      const rare = shared.reduce((n, t) => n + 1 / (uses.get(t) ?? 1), 0);
      const t = names.get(id)!;
      return { id, name: t.name, pattern: t.pattern, code: codes.get(t.pattern)?.code, color: codes.get(t.pattern)?.color ?? "var(--acc)", shared, rare, ps };
    })
    .filter((n) => n.shared.length > 0)
    .sort((a, b) => b.shared.length - a.shared.length || b.rare - a.rare)
    .slice(0, 5);
  const all = scored.flatMap((n) => n.ps);
  return { list: scored, top: scored[0]?.shared.length ?? 0, compared: Math.max(0, byTech.size - 1), topics: mine.size, solved: all.filter((p) => p.state.solved).length, total: all.length };
}

/** The panel beside the cards: how far you are in the pattern, a search box, a filter, every group with how far you are in it, and the
 *  techniques of the group being read. It folds to a column of rings. */
function Rail({ pattern, sections, cur, open, query, setQuery, onPick, onGroup, openAll, foldAll, mini, setMini, find }: { pattern: string; sections: { name: string; items: Item[] }[]; cur: number; open: Set<string>; query: string; setQuery: (q: string) => void; onPick: (i: Item) => void; onGroup: (g: number) => void; openAll: () => void; foldAll: () => void; mini: boolean; setMini: (m: boolean) => void; find: React.RefObject<HTMLInputElement | null> }) {
  const [shown, setShown] = useState<Set<number>>(() => new Set([0]));
  const [filter, setFilter] = useState<"all" | "todo" | "due">("all");
  const q = query.trim().toLowerCase();
  const all = sections.flatMap((s) => s.items);
  const total = all.length;
  const done = all.filter((i) => status(i) === "s").length;
  const started = all.filter((i) => status(i) === "p").length;
  const dueGroups = sections.filter((s) => s.items.some((i) => i.due > 0)).length;
  const keep = (i: Item) => (q ? i.name.toLowerCase().includes(q) : true) && (filter === "todo" ? status(i) !== "s" : filter === "due" ? i.due > 0 : true);
  const lists = all.reduce((n, i) => n + i.lists, 0);
  const solved = all.reduce((n, i) => n + i.solved, 0);
  const seg = (n: number) => (total ? `${(n / total) * 100}%` : "0%");
  const shownCount = sections.reduce((n, s) => n + s.items.filter(keep).length, 0);
  return (
    <>
      <div className="lr-top">
        <div className="lr-t1"><span className="lr-pat">{pattern.toUpperCase()}</span>
          <button className="lr-col" onClick={() => setMini(!mini)} aria-label={mini ? "Expand the panel" : "Collapse the panel"} title={mini ? "Expand the panel" : "Collapse the panel"}>
            <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M10 3 5 8l5 5" /></svg>
          </button>
        </div>
        <div className="lr-sum">
          <i className="lr-bigring" style={{ "--p": lists ? Math.round((solved / lists) * 100) : 0 } as CSSProperties} aria-hidden="true"><b>{solved}/{lists}</b></i>
          <div><b>In the lists</b><small>{total} techniques · {sections.length} group{sections.length === 1 ? "" : "s"}</small></div>
        </div>
        <div className="lr-stack" role="img" aria-label={`${done} practised, ${started} started, ${total - done - started} to go`}><i className="s1" style={{ width: seg(done) }} /><i className="s2" style={{ width: seg(started) }} /></div>
        <div className="lr-leg"><span><i className="s1" />{done} practised</span><span><i className="s2" />{started} started</span><span><i />{total - done - started} to go</span></div>
      </div>
      <div className="lr-find"><input ref={find} type="search" value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Find a technique…" aria-label="Find a technique" autoComplete="off" /><kbd>/</kbd></div>
      <div className="lr-filt" role="radiogroup" aria-label="Show">
        {([["all", "All"], ["todo", "To do"], ["due", "Due"]] as const).map(([k, l]) => (
          <button key={k} role="radio" aria-checked={filter === k} className={filter === k ? "on" : ""} onClick={() => setFilter(k)}>{l}{k === "due" && dueGroups > 0 && <u aria-hidden="true" />}</button>
        ))}
      </div>
      <div className="lr-tools"><span>{q || filter !== "all" ? `${shownCount} of ${total}` : total} TECHNIQUES</span><span><button onClick={openAll}>open all</button> · <button onClick={foldAll}>fold all</button></span></div>
      {sections.map((s, g) => {
        const items = s.items.filter(keep);
        if ((q || filter !== "all") && items.length === 0) return null;
        const isOpen = q || filter !== "all" ? true : shown.has(g) || g === cur;
        const groupDue = s.items.reduce((n, i) => n + i.due, 0);
        const gs = s.items.reduce((n, i) => n + i.solved, 0);
        const gl = s.items.reduce((n, i) => n + i.lists, 0);
        return (
          <div key={s.name} className={`lr-grp${isOpen ? " open" : ""}${g === cur ? " cur" : ""}`}>
            <button className="lr-gb" aria-expanded={isOpen} title={s.name} onClick={() => { setShown((x) => { const n = new Set(x); if (!n.delete(g)) n.add(g); return n; }); onGroup(g); }}>
              <i className="lr-ring" style={{ "--p": ratio(s.items) } as CSSProperties} aria-hidden="true" />
              <span>{s.name}</span>
              <em>{groupDue > 0 && <u title={`${groupDue} due`} aria-hidden="true" />}{gl > 0 ? `${gs}/${gl}` : s.items.length}</em>
              <svg viewBox="0 0 8 12" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M1.5 1.5L6 6l-4.5 4.5" /></svg>
            </button>
            {isOpen && (
              <div className="lr-tl">
                {items.map((i) => <button key={i.id} title={i.name} className={open.has(i.id) ? "cur" : ""} onClick={() => onPick(i)}><b className={status(i)} aria-hidden="true" /><span>{i.name}</span>{i.due > 0 && <u aria-hidden="true" />}</button>)}
              </div>
            )}
          </div>
        );
      })}
      <p className="lr-keys"><kbd>j</kbd> <kbd>k</kbd> move · <kbd>Enter</kbd> open · <kbd>o</kbd> fold · <kbd>/</kbd> find</p>
    </>
  );
}

/** A technique as a folded card: its name, how you are doing, and how many problems; open, it shows the implementation and the problems. */
function Card({ item, open, toggle, today, glyph }: { item: Item; open: boolean; toggle: () => void; today: string; glyph?: string }) {
  const id = anchor(item.id);
  const [opened, setOpened] = useState(open);
  const head = useRef<HTMLButtonElement>(null);
  useEffect(() => { if (open) setOpened(true); }, [open]);
  // once the header is pinned under the group heading it gets a hairline, so the code scrolling beneath it reads as beneath
  useEffect(() => {
    if (!open) return;
    const on = () => { const h = head.current; if (h) h.classList.toggle("stuck", h.getBoundingClientRect().top - h.parentElement!.getBoundingClientRect().top > 2); };
    on();
    window.addEventListener("scroll", on, { passive: true });
    return () => window.removeEventListener("scroll", on);
  }, [open]);
  const first = item.lesson?.signals[0];
  return (
    <article className={`lr-card${open ? " open" : ""}`} id={id}>
      <button ref={head} className={`lr-head${glyph ? " thumb" : ""}`} aria-expanded={open} aria-controls={`${id}-body`} onClick={toggle}>
        <i className={`lr-dot ${status(item)}`} aria-hidden="true" />
        {glyph && <span className="lr-thumb tint" aria-hidden="true"><IdeaGlyph kind={glyph} /></span>}
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
      <i className="lr-ul" aria-hidden="true" />
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
