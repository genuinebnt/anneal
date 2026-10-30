import { useMutation, useQueryClient } from "@tanstack/react-query";
import { marked } from "marked";
import { Link } from "@tanstack/react-router";
import { useRef, useState, type ReactNode } from "react";
import { api, type DsaCompany, type DsaOverview, type DsaProblem, type Grade } from "../api";
import { DIFF, markOf, niceDate } from "../dsa";
import { useShowIdeas } from "../spoilers";
import { toast, updateToast } from "./toasts";

const SAID: Record<Grade, string> = { good: "on your own", hard: "with help", again: "attempted", easy: "instant" };

/** Logs a problem and says when it comes back, in the one toast stack (`toasts.tsx`). `toast` is kept so pages can still render it; it is empty. */
export function useLogger(today: string | undefined): { log: (p: DsaProblem, grade: Grade, practice?: boolean, hints?: number) => void; reset: (p: DsaProblem) => void; undo: () => void; toast: ReactNode; busy: boolean } {
  const qc = useQueryClient();
  // The button of the toast that is showing (Undo after a log, Restore after a reset), so a key can press it.
  const pending = useRef<{ run: () => void; until: number } | null>(null);
  const say = (m: string, act: { label: string; run: () => void } | null = null, extra?: ReactNode, kind: "ok" | "er" = "ok") => {
    pending.current = act ? { run: act.run, until: Date.now() + 9000 } : null;
    return toast(kind, m, undefined, act ?? undefined, extra);
  };
  const refresh = (keys: string[]) => keys.forEach((key) => void qc.invalidateQueries({ queryKey: [key] }));
  const ALL = ["dsa", "dsa-row", "dsa-preview", "practice", "activity", "tracks", "reviews", "dsa-review", "progress", "stats"];
  const m = useMutation({
    mutationFn: ({ p, grade, practice, hints }: { p: DsaProblem; grade: Grade; practice: boolean; hints: number }) => api.logDsa(p.id, grade, practice, hints),
    onSuccess: (r, { p, grade, practice }) => {
      // After Attempted or With help: optionally say why (one click). It is kept with the attempt.
      const why =
        !practice && (grade === "again" || grade === "hard") ? (
          <span className="d-why" role="group" aria-label="Why did it go badly?">
            Why?
            {(["idea", "implementation", "edge_cases", "complexity"] as const).map((w) => (
              <button key={w} onClick={() => void api.setWhy(p.id, w).then(() => say(`${p.title}: noted, ${w.replace("_", " ")}.`), () => undefined)}>
                {w.replace("_", " ")}
              </button>
            ))}
          </span>
        ) : undefined;
      const id = say(
        practice
          ? `${p.title}: practice, ${SAID[grade]}. ${grade === "again" ? "Logged as a miss." : "Counts toward your streak."} The review schedule is unchanged.`
          : r.due
            ? `${p.title}: ${SAID[grade]}. Back for review ${today ? niceDate(r.due, today) : r.due}.`
            : `${p.title}: ${SAID[grade]}. Practice schedules no review.`,
        r.undoable ? { label: "Undo", run: () => u.mutate(p) } : null,
        why,
      );
      refresh(["activity", "tracks", "reviews", "progress", "stats", "dsa-preview", "practice"]);
      // After the overview is fresh, say which problem is next (never the one just logged)
      if (!practice) {
        void qc.refetchQueries({ queryKey: ["dsa"] }).then(() => {
          const o = qc.getQueryData<DsaOverview>(["dsa"]);
          const nid = o?.plan.next_up.find((x) => x !== p.id);
          const n = nid ? o?.problems.find((x) => x.id === nid) : undefined;
          if (n)
            updateToast(id, {
              extra: (
                <>
                  {why}
                  <Link className="d-next" to="/d/$slug" params={{ slug: n.slug }}>
                    Next up: {n.title} ›
                  </Link>
                </>
              ),
            });
        });
      } else refresh(["dsa"]);
    },
    onError: (e: Error) => say(`Couldn't log it: ${e.message}`, null, undefined, "er"),
  });
  const r = useMutation({
    mutationFn: (p: DsaProblem) => api.resetDsa(p.id),
    onSuccess: (out, p) => {
      say(`${p.title}: progress reset.`, out.archived ? { label: "Restore", run: () => rs.mutate(p) } : null);
      refresh(ALL);
    },
    onError: (e: Error) => say(`Couldn't reset it: ${e.message}`, null, undefined, "er"),
  });
  const rs = useMutation({
    mutationFn: (p: DsaProblem) => api.restoreDsa(p.id),
    onSuccess: (out, p) => {
      say(out.restored ? `${p.title}: progress restored.` : `${p.title}: nothing to restore, it has been logged since.`);
      refresh(ALL);
    },
    onError: (e: Error) => say(`Couldn't restore it: ${e.message}`, null, undefined, "er"),
  });
  const u = useMutation({
    mutationFn: (p: DsaProblem) => api.undoDsa(p.id),
    onSuccess: (r, p) => {
      say(r.undone ? `${p.title}: log undone.` : `${p.title}: that log can't be undone. Use Reset progress on its page.`);
      refresh(ALL);
    },
    onError: (e: Error) => say(`Couldn't undo it: ${e.message}`, null, undefined, "er"),
  });
  return {
    log: (p, grade, practice = false, hints = 0) => m.mutate({ p, grade, practice, hints }),
    reset: (p) => r.mutate(p),
    // the toast's own button (Undo after a log, Restore after a reset), for a key
    undo: () => {
      if (pending.current && pending.current.until > Date.now()) pending.current.run();
    },
    busy: m.isPending || r.isPending || u.isPending || rs.isPending,
    toast: null,
  };
}

/** The state icon of a problem: a filled disc with a check (solved on my own), a half-filled ring (solved with help),
 *  a yellow ring with an exclamation mark (attempted), a violet ring with a clock when a review is due, and a dashed ring when not started. */
export function Mark({ p, today }: { p: DsaProblem; today: string }) {
  const mark = markOf(p);
  const started = !!p.state.last_grade;
  const due = started && !!p.state.due && p.state.due <= today && mark !== "fail";
  const label = started ? (due ? "Due for review" : mark === "fail" ? "Attempted" : mark === "help" ? "Solved with help" : "Solved") : "Not started";
  const kind = due ? "due" : mark || "none";
  return (
    <span className={`d-st ${mark} k-${kind}`} title={label} role="img" aria-label={label}>
      <svg viewBox="0 0 24 24" fill="none" aria-hidden="true">
        <circle className="r" cx="12" cy="12" r="10" />
        {kind === "solo" && <path className="g" d="M7.2 12.4l3.2 3.2 6.4-6.8" />}
        {kind === "help" && <path className="h" d="M12 2a10 10 0 0 0 0 20z" />}
        {kind === "fail" && <path className="g" d="M12 6.8v6M12 16.6v.5" />}
        {kind === "due" && <path className="g" d="M12 6.8V12l3.4 2" />}
      </svg>
    </span>
  );
}

/** What the round state icons of a problem mean, once, above a list of them. */
export function StateLegend() {
  const items: [string, string][] = [
    ["none", "Not started"],
    ["solo", "Solved on my own"],
    ["help", "Solved with help"],
    ["fail", "Attempted"],
    ["due", "Review due"],
  ];
  return (
    <div className="d-legend" aria-label="What the icons mean">
      {items.map(([kind, label]) => (
        <span key={kind}>
          <span className={`d-st ${kind === "due" || kind === "none" ? "" : kind} k-${kind}`} aria-hidden="true">
            <svg viewBox="0 0 24 24" fill="none">
              <circle className="r" cx="12" cy="12" r="10" />
              {kind === "solo" && <path className="g" d="M7.2 12.4l3.2 3.2 6.4-6.8" />}
              {kind === "help" && <path className="h" d="M12 2a10 10 0 0 0 0 20z" />}
              {kind === "fail" && <path className="g" d="M12 6.8v6M12 16.6v.5" />}
              {kind === "due" && <path className="g" d="M12 6.8V12l3.4 2" />}
            </svg>
          </span>
          {label}
        </span>
      ))}
    </div>
  );
}

/** Company pills: the ones asked in the last six months highlighted, and "+n" opens the rest in place.
 *  With `onPick`, a pill filters the list by that company. */
export function Companies({ companies, limit = 5, onPick, picked }: { companies: DsaCompany[]; limit?: number; onPick?: (name: string) => void; picked?: ReadonlySet<string> }) {
  const [all, setAll] = useState(false);
  if (!companies.length) return null;
  const shown = all ? companies : companies.slice(0, limit);
  const rest = companies.length - limit;
  return (
    <div className="d-cos">
      {shown.map((c) => {
        const cls = `d-co${c.recent ? " hot" : ""}${picked?.has(c.name) ? " on" : ""}`;
        const title = `${c.recent ? `${c.name}: asked in the last six months` : c.name}${onPick ? " · click to filter" : ""}`;
        return onPick ? (
          <button key={c.name} className={cls} title={title} onClick={() => onPick(c.name)}>
            {c.name}
          </button>
        ) : (
          <span key={c.name} className={cls} title={title}>
            {c.name}
          </span>
        );
      })}
      {rest > 0 && (
        <button className="d-co more" aria-expanded={all} title={all ? "Show fewer" : companies.slice(limit).map((c) => c.name).join(", ")} onClick={() => setAll(!all)}>
          {all ? "show fewer" : `+${rest}`}
        </button>
      )}
    </div>
  );
}

/** Markdown from the content files (ours, so it's trusted), as inline or block HTML. */
export function Md({ text, inline }: { text: string; inline?: boolean }) {
  const html = inline ? marked.parseInline(text, { async: false }) : marked.parse(text, { async: false });
  return inline ? <span dangerouslySetInnerHTML={{ __html: html }} /> : <div className="d-md" dangerouslySetInnerHTML={{ __html: html }} />;
}

const KEYWORDS = "def|class|return|if|elif|else|for|while|in|not|and|or|is|None|True|False|import|from|as|with|yield|lambda|try|except|finally|raise|pass|break|continue|global|nonlocal";
const BUILTINS = "range|len|set|dict|list|tuple|deque|sum|any|all|min|max|enumerate|zip|sorted|print|int|str|self|Counter|defaultdict";
const TOKEN = new RegExp(`(#[^\\n]*)|("""[\\s\\S]*?"""|"(?:[^"\\\\\\n]|\\\\.)*"|'(?:[^'\\\\\\n]|\\\\.)*')|\\b(${KEYWORDS})\\b|\\b(${BUILTINS})\\b|\\b(\\d[\\d_]*)\\b|(\\w+)(?=\\()`, "g");

/** Python text as highlighted spans (the classes `py-k`, `py-s`, `py-c`, `py-b`, `py-n`, `py-f`), for anything that draws code its own way. */
export function highlightPy(code: string): ReactNode[] {
  const parts: ReactNode[] = [];
  let last = 0;
  for (const m of code.matchAll(TOKEN)) {
    const at = m.index ?? 0;
    if (at > last) parts.push(code.slice(last, at));
    const [text, comment, str, kw, builtin, num] = m;
    const cls = comment ? "c" : str ? "s" : kw ? "k" : builtin ? "b" : num ? "n" : "f";
    parts.push(<span key={at} className={`py-${cls}`}>{text}</span>);
    last = at + text.length;
  }
  parts.push(code.slice(last));
  return parts;
}

/** Python with light highlighting and a copy button. */
export function PyCode({ code, lineNumbers = false }: { code: string; lineNumbers?: boolean }) {
  const [copied, setCopied] = useState(false);
  const parts = highlightPy(code);
  return (
    <div className="d-code">
      <button
        className="d-copy"
        onClick={() => {
          navigator.clipboard.writeText(code).then(
            () => {
              setCopied(true);
              window.setTimeout(() => setCopied(false), 1600);
            },
            () => undefined,
          );
        }}
      >
        {copied ? "copied" : "copy"}
      </button>
      <div className="d-codebody">
        {lineNumbers && (
          <div className="d-ln" aria-hidden="true">
            {code.replace(/\n$/, "").split("\n").map((_, i) => i + 1).join("\n")}
          </div>
        )}
        <pre>{parts}</pre>
      </div>
    </div>
  );
}

export const LIST_LABEL: Record<NonNullable<DsaProblem["list_tag"]>, string> = {
  blind75: "BLIND 75",
  neetcode150: "NEETCODE 150",
  neetcode250: "NEETCODE 250",
  all: "NEETCODE ALL",
};
export const PRIORITY_LABEL: Record<DsaProblem["priority"], string> = { must: "MUST", strong: "STRONG", practice: "PRACTICE", warmup: "WARM-UP" };

/** The tags of a problem row, always in the same order (docs/DSA_LEARN_PAGE_SPEC.md, section 3): difficulty, priority, the narrowest
 *  NeetCode list (nothing for a problem outside the lists), recent, whether the site has a written solution, premium. */
export function ProblemTags({ p }: { p: DsaProblem }) {
  return (
    <span className="pl-tags">
      <span className="d-bdg lv" style={{ color: DIFF[p.difficulty][1] }}>{DIFF[p.difficulty][0]}</span>
      <span className={`d-bdg pri-${p.priority}`} title={p.priority === "must" ? "Must solve" : p.priority === "strong" ? "Strong: asked often" : p.priority === "warmup" ? "A warm-up" : "Practice"}>
        {PRIORITY_LABEL[p.priority]}
      </span>
      {p.list_tag && <span className="d-bdg lst" title="The narrowest NeetCode list this problem is in">{LIST_LABEL[p.list_tag]}</span>}
      {p.recent && <span className="d-bdg rec" title="Asked in the last six months">RECENT</span>}
      {p.has_page && <span className="d-bdg sol" title="This site has a written solution for it">SOLUTION</span>}
      {p.premium && <span className="d-bdg prem">PREM</span>}
    </span>
  );
}

/** A problem under a technique: the mark, the title (it opens the site's problem page, which links to LeetCode), the tags, and the
 *  companies and LeetCode topics that are asked about it. */
/** The key insight of a problem, one line (docs/DSA_LEARN_PAGE_SPEC.md section 8). */
export function Insight({ text }: { text: string }) {
  const [show] = useShowIdeas();
  const [open, setOpen] = useState(false);
  if (!text) return null;
  // the idea is the solution's first move, so it stays hidden until asked for
  return show || open ? (
    <p className="d-ins">{text}</p>
  ) : (
    <button className="d-ins d-ins-hide" onClick={(e) => { e.preventDefault(); setOpen(true); }} aria-label="Show the key idea">
      Key idea hidden · <span>show</span>
    </button>
  );
}

export function ProblemLine({ p, today }: { p: DsaProblem; today: string }) {
  return (
    <div className={`pl pl-${p.priority}${p.list_tag ? "" : " pl-out"}`}>
      <Link to="/d/$slug" params={{ slug: p.slug }} className="pl-top">
        <Mark p={p} today={today} />
        <span className="pl-title">
          <span className="pl-num">#{p.number}</span> {p.title}
        </span>
        <ProblemTags p={p} />
      </Link>
      <Insight text={p.insight} />
      <span className="pl-sub">
        {p.companies.slice(0, 3).map((c) => (
          <span key={c.name} className={`d-co${c.recent ? " hot" : ""}`} title={c.recent ? `${c.name}: asked in the last six months` : c.name}>{c.name}</span>
        ))}
        {p.companies.length > 3 && <span className="d-co more" title={p.companies.slice(3).map((c) => c.name).join(", ")}>+{p.companies.length - 3}</span>}
        <span className="pl-topics">{p.tags.slice(0, 3).join(" · ")}</span>
      </span>
    </div>
  );
}
