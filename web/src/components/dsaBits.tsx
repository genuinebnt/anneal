import { useMutation, useQueryClient } from "@tanstack/react-query";
import { marked } from "marked";
import { Link } from "@tanstack/react-router";
import { useEffect, useRef, useState, type ReactNode } from "react";
import { api, type DsaCompany, type DsaProblem, type Grade } from "../api";
import { DIFF, markOf, niceDate } from "../dsa";

const SAID: Record<Grade, string> = { good: "on your own", hard: "with help", again: "not yet", easy: "easy" };

/** Logs a problem and says when it comes back. Render `toast` once on the page. */
export function useLogger(today: string | undefined): { log: (p: DsaProblem, grade: Grade, practice?: boolean) => void; toast: ReactNode; busy: boolean } {
  const qc = useQueryClient();
  const [message, setMessage] = useState<string | null>(null);
  const timer = useRef<number>(0);
  const say = (m: string) => {
    setMessage(m);
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => setMessage(null), 3200);
  };
  useEffect(() => () => window.clearTimeout(timer.current), []);
  const m = useMutation({
    mutationFn: ({ p, grade, practice }: { p: DsaProblem; grade: Grade; practice: boolean }) => api.logDsa(p.id, grade, practice),
    onSuccess: (r, { p, grade, practice }) => {
      say(
        practice
          ? `${p.title}: practice, ${SAID[grade]}. ${grade === "again" ? "Logged as a miss." : "Counts toward your streak."} The review schedule is unchanged.`
          : r.due
            ? `${p.title}: ${SAID[grade]}. Back for review ${today ? niceDate(r.due, today) : r.due}.`
            : `${p.title}: ${SAID[grade]}. Practice schedules no review.`,
      );
      for (const key of ["dsa", "dsa-preview", "practice", "activity", "tracks", "reviews", "progress", "stats"]) void qc.invalidateQueries({ queryKey: [key] });
    },
    onError: (e: Error) => say(`Couldn't log it: ${e.message}`),
  });
  return {
    log: (p, grade, practice = false) => m.mutate({ p, grade, practice }),
    busy: m.isPending,
    toast: message ? (
      <div className="d-toast" role="status">
        {message}
      </div>
    ) : null,
  };
}

/** The state icon of a problem: a filled disc with a check (solved on my own), a half-filled ring (solved with help),
 *  a red ring with a cross (couldn't yet), a violet ring with a clock when a review is due, and a dashed ring when not started. */
export function Mark({ p, today }: { p: DsaProblem; today: string }) {
  const mark = markOf(p);
  const started = !!p.state.last_grade;
  const due = started && !!p.state.due && p.state.due <= today && mark !== "fail";
  const label = started ? (due ? "Due for review" : mark === "fail" ? "Couldn't yet" : mark === "help" ? "Solved with help" : "Solved") : "Not started";
  const kind = due ? "due" : mark || "none";
  return (
    <span className={`d-st ${mark} k-${kind}`} title={label} role="img" aria-label={label}>
      <svg viewBox="0 0 24 24" fill="none" aria-hidden="true">
        <circle className="r" cx="12" cy="12" r="10" />
        {kind === "solo" && <path className="g" d="M7.2 12.4l3.2 3.2 6.4-6.8" />}
        {kind === "help" && <path className="h" d="M12 2a10 10 0 0 0 0 20z" />}
        {kind === "fail" && <path className="g" d="M8.4 8.4l7.2 7.2M15.6 8.4l-7.2 7.2" />}
        {kind === "due" && <path className="g" d="M12 6.8V12l3.4 2" />}
      </svg>
    </span>
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

/** Python with light highlighting and a copy button. */
export function PyCode({ code }: { code: string }) {
  const [copied, setCopied] = useState(false);
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
      <pre>{parts}</pre>
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
  return text ? <p className="d-ins">{text}</p> : null;
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
