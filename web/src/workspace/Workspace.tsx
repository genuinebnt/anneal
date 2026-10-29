import { useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { Skeleton } from "../components/Skeleton";
import { Link, useNavigate } from "@tanstack/react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { marked } from "marked";
import { ApiError, api, type Diagnostic, type ProblemDetail, type RunOutcome, type RunView, type ScratchResult, type TestOutcome } from "../api";
import { Header } from "../components/Header";
import { Celebration, celebrateOff } from "../components/kit";
import { BAND_LABEL, LEVEL_COLOR, modeColor, pad2 } from "../components/bits";
import { NAV_SECTIONS, SECTION_NAMES, type NavArea } from "../curriculum";
import { Editor, GOTO_EVENT } from "./Editor";
import { EditorSettingsButton } from "./EditorSettings";
import { useEditorSettings } from "../settings";
import { changedLines } from "./diff";
import { deriveLanes } from "./lanes";
import { FocusTimer } from "../components/FocusTimer";
import { Ansi } from "./ansi";
import { type TestCase, pythonTestCases, testCases } from "./testcases";
import { type RaSession, type RaStatus, connectRa } from "./lsp";

type LeftTab = "problem" | "hints" | "solution" | "related";
type ConsoleTab = "compiler" | "output" | "timeline";

const CONSOLE_MIN = 120;

/** Interview-realistic budgets from CURRICULUM.md, in minutes. */
const BUDGET = { write: { easy: 10, medium: 25, hard: 40 }, fix: { easy: 5, medium: 10, hard: 15 }, stage: { easy: 60, medium: 60, hard: 90 } } as const;

const md = (s: string) => marked.parse(s, { async: false });
const mmss = (s: number) => `${pad2(Math.floor(s / 60))}:${pad2(s % 60)}`;
const pyTestNames = (src: string) => [...src.matchAll(/^def test_(\w+)/gm)].map((m) => m[1]!);
const testNames = (src: string) => [...src.matchAll(/#\[test\]\s*(?:#\[[^\]]*\]\s*)*fn\s+(\w+)/g)].map((m) => m[1]!);
const areaOf = (section: string): NavArea => (Object.keys(NAV_SECTIONS) as NavArea[]).find((a) => (NAV_SECTIONS[a] as readonly string[]).includes(section)) ?? "dsa";

function runLabel(r: RunView): [string, string] {
  if (r.status !== "compile_error" && r.violations.length) return [`rule: ${r.violations[0]!.rule}`, "var(--warn)"];
  switch (r.status) {
    case "passed":
      return [`${r.passed} / ${r.total} ok`, "var(--grn)"];
    case "failed":
      return [`${r.passed} / ${r.total}`, "var(--acc)"];
    case "compile_error":
      return [r.diagnostics.find((d) => d.level === "error")?.code ?? "error", "var(--bad)"];
    case "timeout":
      return ["timeout", "var(--bad)"];
  }
}

export function Workspace({ id }: { id: string }) {
  const q = useQuery({ queryKey: ["problem", id], queryFn: () => api.problem(id), retry: (n, e) => !(e instanceof ApiError && e.status === 423) && n < 2 });
  if (q.error instanceof ApiError && q.error.status === 423) {
    return (
      <>
        <Header area="dsa" compact />
        <main className="page">
          <div className="wrap" style={{ maxWidth: 640, paddingBlock: 48 }}>
            <div className="dash" style={{ display: "flex", flexDirection: "column", gap: 12 }}>
              <div className="lab" style={{ color: "var(--warn)" }}>
                LOCKED PRACTICE PROBLEM
              </div>
              <p style={{ margin: 0, color: "var(--fg)" }}>{q.error.message}.</p>
              <p className="note" style={{ margin: 0 }}>
                Practice problems open once you've logged their LeetCode problem, with any grade. Some also open while that problem is coming up next in your plan.
              </p>
              <Link to="/dsa" style={{ color: "var(--acc)" }}>
                Back to DSA →
              </Link>
            </div>
          </div>
        </main>
      </>
    );
  }
  if (!q.data) {
    return (
      <>
        <Header area="dsa" compact />
        <main className="page">
          <div className="wrap">
            {q.isError ? <p className="notice bad">{`Couldn't load ${id}: ${(q.error as Error).message}`}</p> : <Skeleton lines={6} />}
          </div>
        </main>
      </>
    );
  }
  return <Loaded key={id} p={q.data} />;
}

function Loaded({ p }: { p: ProblemDetail }) {
  const { editor: editorSettings, set: saveEditor } = useEditorSettings();
  // Python practice problems have no scratch file, language server, formatter or lints: the editor and panels adapt.
  const py = p.language === "python";
  const ext = py ? "py" : "rs";
  useFocusHeartbeat(p.id);
  const qc = useQueryClient();
  const [code, setCode] = useState(p.draft ?? p.starter);
  const [docKey, setDocKey] = useState(`${p.id}:0`);
  const [left, setLeft] = useState<LeftTab>("problem");
  const [consoleTab, setConsoleTab] = useState<ConsoleTab>("compiler");
  const [consoleOpen, setConsoleOpen] = useState(false);
  const [consoleH, setConsoleH] = useStoredNumber("anneal-console-height", 260);
  const [leftW, setLeftW] = useStoredNumber("anneal-left-width", 360);
  const [rightW, setRightW] = useStoredNumber("anneal-right-width", 340);
  // Under 900 px the panes (problem, code, tests) are shown one at a time, whatever the saved layout says.
  const [narrow, setNarrow] = useState(() => window.matchMedia("(max-width: 900px)").matches);
  useEffect(() => {
    const m = window.matchMedia("(max-width: 900px)");
    const f = () => setNarrow(m.matches);
    m.addEventListener("change", f);
    return () => m.removeEventListener("change", f);
  }, []);
  const [mobilePane, setMobilePane] = useState<"l" | "c" | "r">("l");
  const [leftOpen, setLeftOpen] = useStoredNumber("anneal-left-open", 1);
  const [rightOpen, setRightOpen] = useStoredNumber("anneal-right-open", 1);
  const [file, setFile] = useState<"lib" | "main" | "tests" | "hidden" | "solution">("lib");
  const [main, setMain] = useState(p.scratch);
  const [scratchOut, setScratchOut] = useState<ScratchResult | null>(null);
  const [lastAction, setLastAction] = useState<"tests" | "scratch">("tests");
  // The status-bar toggles are saved editor settings, so they carry across problems and reloads.
  const autocomplete = editorSettings.autocomplete;
  const setAutocomplete = (on: boolean) => saveEditor({ ...editorSettings, autocomplete: on });
  const borrowish = !py && p.mode === "fix" || p.tags.some((t) => /^E0[45]\d\d$/.test(t) || /borrow/.test(t));
  const lanesOn = editorSettings.borrow_lanes && !py;
  const setLanesOn = (on: boolean) => saveEditor({ ...editorSettings, borrow_lanes: on });
  const [selected, setSelected] = useState<number | null>(null);
  const nav = useNavigate();
  // A Submit that passes: the popup offers the next problem or staying here.
  const [win, setWin] = useState<{ passed: number; total: number; ms: number; hints: number; assisted: boolean } | null>(null);
  const [open, setOpen] = useState<string | null>(null);
  const [cursor, setCursor] = useState([1, 1]);
  const [confirm, setConfirm] = useState<"reset" | "solution" | null>(null);
  const [elapsed, setElapsed] = useState(0);
  const ra = editorSettings.rust_analyzer;
  const liveClippy = editorSettings.live_clippy;
  // A failed ⌘S / ⇧⌥F format (rustfmt couldn't parse the code), shown in the status bar for a few seconds.
  const [formatError, setFormatError] = useState<string | null>(null);
  useEffect(() => {
    if (!formatError) return;
    const t = setTimeout(() => setFormatError(null), 5000);
    return () => clearTimeout(t);
  }, [formatError]);
  const setRa = (on: boolean) => saveEditor({ ...editorSettings, rust_analyzer: on });
  const [raStatus, setRaStatus] = useState<RaStatus>("off");
  const [raDetail, setRaDetail] = useState<string | undefined>();
  const [raSession, setRaSession] = useState<RaSession | null>(null);
  const [raDiag, setRaDiag] = useState({ errors: 0, warnings: 0 });
  const [raEpoch, setRaEpoch] = useState(0);

  // rust-analyzer: one session per open problem, closed when toggled off or on leaving.
  useEffect(() => {
    if (!ra || py) {
      setRaStatus("off");
      return;
    }
    const session = connectRa(p.id, {
      status: (s, detail) => {
        setRaStatus(s);
        setRaDetail(detail);
        if (s === "ready") setRaEpoch((n) => n + 1);
      },
      diagnostics: (errors, warnings) => setRaDiag({ errors, warnings }),
    }, liveClippy);
    session.ready.then(
      () => setRaSession(session),
      () => {},
    );
    return () => {
      session.close();
      setRaSession(null);
      setRaDiag({ errors: 0, warnings: 0 });
    };
  }, [ra, py, liveClippy, p.id]);

  // Save to the server a second after typing stops so rust-analyzer's cargo check sees borrow errors.
  useEffect(() => {
    if (!raSession) return;
    const t = setTimeout(() => raSession.save(code), 1000);
    return () => clearTimeout(t);
  }, [code, raSession]);

  useEffect(() => {
    const start = Date.now();
    const t = setInterval(() => setElapsed(Math.floor((Date.now() - start) / 1000)), 1000);
    return () => clearInterval(t);
  }, []);

  // Autosave the buffer shortly after typing stops.
  const saved = useRef(code);
  useEffect(() => {
    if (code === saved.current) return;
    const t = setTimeout(() => {
      saved.current = code;
      api.saveDraft(p.id, code).catch(() => {});
    }, 800);
    return () => clearTimeout(t);
  }, [code, p.id]);

  // The scratch main.rs autosaves the same way.
  const savedMain = useRef(main);
  useEffect(() => {
    if (py || main === savedMain.current) return;
    const t = setTimeout(() => {
      savedMain.current = main;
      api.saveScratch(p.id, main).catch(() => {});
    }, 800);
    return () => clearTimeout(t);
  }, [main, py, p.id]);

  /** Jumps from a diagnostic's location to that line in the right editor tab. */
  const goto = (path: string, line: number, col: number) => {
    const key = path === "src/bin/scratch.rs" ? "main" : path === "tests/hidden.rs" ? "hidden" : path.startsWith("tests/") ? "tests" : "lib";
    setFile(key);
    setTimeout(() => window.dispatchEvent(new CustomEvent(GOTO_EVENT, { detail: { key, line, col } })), 60);
  };
  const toggleLayout = (which: Region) => {
    if (which === "left") setLeftOpen(leftOpen ? 0 : 1);
    else if (which === "right") setRightOpen(rightOpen ? 0 : 1);
    else setConsoleOpen(!consoleOpen);
  };
  const toggleRef = useRef(toggleLayout);
  toggleRef.current = toggleLayout;
  useEffect(() => {
    // ⌘B problem panel, ⌘J console, ⌥⌘B tests panel (Ctrl on other platforms), like VS Code.
    const onKey = (e: KeyboardEvent) => {
      if (!(e.metaKey || e.ctrlKey) || e.shiftKey) return;
      const which: Region | null = e.code === "KeyB" ? (e.altKey ? "right" : "left") : e.code === "KeyJ" && !e.altKey ? "bottom" : null;
      if (!which) return;
      e.preventDefault();
      e.stopPropagation();
      toggleRef.current(which);
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);
  const showConsole = (tab: ConsoleTab) => {
    setConsoleTab(tab);
    setConsoleOpen(true);
  };
  const update = (next: ProblemDetail) => qc.setQueryData(["problem", p.id], next);
  const afterRun = (out: RunOutcome) => {
    qc.setQueryData<ProblemDetail>(["problem", p.id], (old) => old && { ...old, runs: [...old.runs, out.run], attempt: out.attempt, solution: out.solution });
    qc.invalidateQueries({ queryKey: ["tracks"] });
    qc.invalidateQueries({ queryKey: ["track"] });
    // Solving unlocks the hidden tests, which only the full problem carries.
    if (out.run.status === "passed" && !p.hidden_tests) qc.invalidateQueries({ queryKey: ["problem", p.id] });
    setSelected(null);
    if (out.run.kind === "submit" && out.run.status === "passed" && !celebrateOff()) {
      setWin({ passed: out.run.passed, total: out.run.total, ms: out.run.duration_ms, hints: out.attempt.hints_revealed, assisted: out.attempt.assisted });
    }
    const firstFail = out.run.tests.find((t) => t.outcome !== "passed");
    setOpen(firstFail ? (firstFail.suite === "hidden" ? "hidden:" : "") + firstFail.name : null);
    setLastAction("tests");
    // The console comes back after every run, on whichever tab has something to say.
    if (out.run.diagnostics.length) showConsole("compiler");
    else if (out.run.tests.some((t) => t.stdout)) showConsole("output");
    else showConsole("compiler");
  };
  const run = useMutation({ mutationFn: () => api.run(p.id, code), onSuccess: afterRun });
  const submit = useMutation({ mutationFn: () => api.submit(p.id, code), onSuccess: afterRun });
  const hint = useMutation({ mutationFn: () => api.revealHint(p.id), onSuccess: update });
  const solution = useMutation({
    mutationFn: () => api.revealSolution(p.id),
    onSuccess: (d) => {
      update(d);
      setConfirm(null);
      setFile("solution");
    },
  });
  const reset = useMutation({
    mutationFn: () => api.reset(p.id),
    onSuccess: (d) => {
      update(d);
      saved.current = d.starter;
      setCode(d.starter);
      setDocKey(`${p.id}:${Date.now()}`);
      setConfirm(null);
    },
  });
  const scratch = useMutation({
    mutationFn: () => api.runScratch(p.id, code, main),
    onSuccess: (out) => {
      setScratchOut(out);
      setLastAction("scratch");
      showConsole(out.status === "compile_error" ? "compiler" : "output");
    },
  });
  const busy = run.isPending || submit.isPending || scratch.isPending;
  const doScratch = () => {
    if (py || busy || p.status !== "ready") return;
    showConsole("output");
    scratch.mutate();
  };
  // Test runs report in the tests panel, so they bring it back if it was hidden.
  const doRun = () => {
    if (busy || p.status !== "ready") return;
    setRightOpen(1);
    run.mutate();
  };
  const doSubmit = () => {
    if (busy || p.status !== "ready") return;
    setRightOpen(1);
    submit.mutate();
  };
  const failure = run.error ?? submit.error ?? scratch.error;

  const runs = p.runs;
  const latest = runs.at(-1);
  const shown = (selected !== null ? runs.find((r) => r.id === selected) : latest) ?? null;
  const shownIdx = shown ? runs.indexOf(shown) : -1;
  const prevRun = shownIdx > 0 ? runs[shownIdx - 1] : undefined;
  const lanes = useMemo(() => (latest && !py ? deriveLanes(latest.diagnostics, latest.code) : null), [latest, py]);
  const lensDiags = useMemo(() => (latest && latest.code === code ? latest.diagnostics : (latest?.diagnostics ?? [])), [latest, code]);
  const names = useMemo(() => (py ? pyTestNames(p.visible_tests) : testNames(p.visible_tests)), [p.visible_tests, py]);
  const cases = useMemo(() => (py ? pythonTestCases(p.visible_tests) : testCases(p.visible_tests)), [p.visible_tests, py]);
  const hiddenCases = useMemo(() => (p.hidden_tests ? (py ? pythonTestCases(p.hidden_tests) : testCases(p.hidden_tests)) : null), [p.hidden_tests, py]);
  const budget = BUDGET[p.mode][p.level];
  const area = areaOf(p.track.section);

  return (
    <>
      <Header area={area} compact />
      {win && (
        <Celebration
          title="Problem solved"
          message={win.assisted ? "All tests pass. Solved with help, so it counts as assisted." : "All tests pass, unassisted."}
          stats={[
            { value: `${win.passed}/${win.total}`, label: "tests" },
            { value: `${(win.ms / 1000).toFixed(1)}s`, label: "run time" },
            { value: String(win.hints), label: "hints" },
          ]}
          next={p.next ? { kicker: "NEXT PROBLEM", title: "Keep going in this track" } : { kicker: "TRACK DONE", title: `Back to ${p.track.name}` }}
          goLabel={p.next ? "Next problem" : `Back to ${p.track.name}`}
          onGo={() => {
            setWin(null);
            if (p.next) nav({ to: "/p/$id", params: { id: p.next } });
            else nav({ to: "/t/$track", params: { track: p.track.slug } });
          }}
          onClose={() => setWin(null)}
        />
      )}
      <main>
        {p.attempt.resolve && (
          <div className="resolve-bar">
            <b>{p.attempt.solved ? "RE-SOLVED" : "RE-SOLVE"}</b>
            <span>
              {p.attempt.solved
                ? p.attempt.assisted
                  ? "Assisted this time: it comes back in 3 days."
                  : "Unassisted: it moves up the review ladder."
                : "From the starter, hints locked again. Solve it unassisted to move it up the review ladder."}
            </span>
          </div>
        )}
        <div className="subbar">
          <div className="crumb">
            <Link to={`/${area}`} style={{ color: modeColor(p.mode) }}>
              {SECTION_NAMES[p.track.section].toUpperCase()}
            </Link>
            <span>/</span>
            <Link to="/t/$track" params={{ track: p.track.slug }}>
              {p.track.code} {p.track.name.toUpperCase()}
            </Link>
            <span>/</span>
            <span>
              <span style={{ color: LEVEL_COLOR[p.stage.band] }}>{BAND_LABEL[p.stage.band]}</span> · {p.stage.name.toUpperCase()}
            </span>
          </div>
          <div className="vr" />
          <span className="wtitle">{p.title}</span>
          <div className="subpills">
            <span className="pill solid" style={{ background: modeColor(p.mode) }}>
              {py ? "PYTHON" : p.mode === "fix" ? "FIX THIS" : p.mode === "stage" ? "STAGE" : "WRITE IT"}
            </span>
            <span className="pill" style={{ borderColor: LEVEL_COLOR[p.level], color: LEVEL_COLOR[p.level] }}>
              {p.level.toUpperCase()}
            </span>
            {p.tags.slice(0, 3).map((t) => (
              <span className="pill" key={t}>
                {t}
              </span>
            ))}
          </div>
          <div className="sbr">
            <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
              {p.prev ? (
                <Link to="/p/$id" params={{ id: p.prev }} style={{ color: "var(--dim)" }} aria-label="Previous problem">
                  ‹
                </Link>
              ) : (
                <span style={{ color: "var(--line)" }}>‹</span>
              )}
              <span>
                {p.position} / {p.count}
              </span>
              {p.next ? (
                <Link to="/p/$id" params={{ id: p.next }} style={{ color: "var(--dim)" }} aria-label="Next problem">
                  ›
                </Link>
              ) : (
                <span style={{ color: "var(--line)" }}>›</span>
              )}
            </div>
            <div className="clock" title={`Interview budget for a ${p.level} ${p.mode === "fix" ? "fix-this" : "problem"}: ${budget} minutes`}>
              <span className="dot" style={{ background: elapsed > budget * 60 ? "var(--bad)" : modeColor(p.mode) }} />
              <small>PRACTICE</small>
              <span style={{ color: "var(--fg)" }}>{mmss(elapsed)}</span>
              <small>/ {budget}:00</small>
            </div>
            <LayoutButtons left={!!leftOpen} bottom={consoleOpen} right={!!rightOpen} toggle={toggleLayout} />
          </div>
        </div>

        {/* on a phone the three panes are one at a time */}
        <div className="ws-tabs" role="tablist" aria-label="Workspace">
          {([["l", "Problem"], ["c", "Code"], ["r", "Tests"]] as const).map(([k, label]) => (
            <button key={k} role="tab" aria-selected={mobilePane === k} className={mobilePane === k ? "on" : ""} onClick={() => setMobilePane(k)}>{label}</button>
          ))}
        </div>
        <div className="ws" data-pane={mobilePane} style={{ "--cols": [leftOpen ? `${leftW}px` : "", "minmax(0, 1fr)", rightOpen ? `${rightW}px` : ""].join(" ").trim() } as CSSProperties}>
          {/* ---------- left: problem, hints, solution ---------- */}
          {leftOpen || narrow ? (
            <section className="pane l">
              <div className="lgrip" onPointerDown={(e) => dragWidth(e, leftW, setLeftW, 1)} title="Drag to resize" aria-hidden="true" />
              <div className="tabs" role="tablist">
                {(
                  [
                    ["problem", "Problem"],
                    ["hints", `Hints ${p.hints.revealed.length}/${p.hints.total}`],
                    ["solution", "Solution"],
                    ["related", py ? "Unlocked by" : "Related"],
                  ] as const
                ).map(([k, label]) => (
                  <button key={k} role="tab" className={left === k ? "on" : ""} aria-selected={left === k} onClick={() => setLeft(k)}>
                    {label}
                  </button>
                ))}
              </div>
              <div className="pbody">
                {left === "problem" && (
                  <div className="stack">
                    <div className="lab">
                      {p.track.code} · {p.stage.name.toUpperCase()} · {p.stage.position} OF {p.stage.count}
                    </div>
                    <h2 className="ptitle">{p.title}</h2>
                    <div className="md" dangerouslySetInnerHTML={{ __html: md(p.statement) }} />
                    {p.rules && (
                      <div className="stack" style={{ gap: 8 }}>
                        <div className="lab">RULES</div>
                        <div className="rules" style={{ fontSize: 13.5, gridTemplateColumns: "14px 1fr", gap: "6px 8px" }}>
                          {p.rules.forbid_methods.map((m) => (
                            <Rule key={m} ok={false}>
                              no <code>.{m}()</code>
                            </Rule>
                          ))}
                          {p.rules.forbid_types.length > 0 && (
                            <Rule ok={false}>
                              no{" "}
                              {p.rules.forbid_types.map((t, i) => (
                                <span key={t}>
                                  {i > 0 && " / "}
                                  <code>{t}</code>
                                </span>
                              ))}
                            </Rule>
                          )}
                          {p.rules.max_changed_lines !== null && <Rule ok>change at most {p.rules.max_changed_lines} lines</Rule>}
                        </div>
                        <p className="note">Checked on every run. A broken rule means a submit doesn't count, even when every test passes.</p>
                      </div>
                    )}
                    {p.examples.map((e, i) => (
                      <div className="stack" style={{ gap: 8 }} key={i}>
                        <div className="lab">EXAMPLE</div>
                        <div className="kv">
                          <span>input</span>
                          <code>{e.input}</code>
                          <span>returns</span>
                          <code style={{ color: "var(--grn)" }}>{e.output}</code>
                        </div>
                      </div>
                    ))}
                    {p.constraints.length > 0 && (
                      <div className="stack" style={{ gap: 8 }}>
                        <div className="lab">CONSTRAINTS</div>
                        <div className="m" style={{ fontSize: 12, lineHeight: 1.8 }}>
                          {p.constraints.map((c) => (
                            <div key={c}>{c}</div>
                          ))}
                        </div>
                      </div>
                    )}
                    {p.teaches.length > 0 && (
                      <div className="stack" style={{ gap: 10, paddingTop: 14, borderTop: "1px solid var(--line2)" }}>
                        <div className="lab">WHAT THIS TEACHES</div>
                        {p.teaches.map((t) => (
                          <div className="bul" key={t} style={{ ["--bul" as string]: modeColor(p.mode) }}>
                            <span className="md-inline" dangerouslySetInnerHTML={{ __html: marked.parseInline(t, { async: false }) }} />
                          </div>
                        ))}
                      </div>
                    )}
                    {p.follow_up && (
                      <div className="dash">
                        <div className="lab" style={{ color: "var(--acc)", marginBottom: 4 }}>
                          {p.mode === "fix" ? "ASKED AS" : "INTERVIEW FOLLOW-UP"}
                        </div>
                        {p.follow_up}
                      </div>
                    )}
                  </div>
                )}

                {left === "hints" && (
                  <div className="stack" style={{ gap: 12 }}>
                    <p className="note">Each hint you open before solving marks the attempt as assisted and brings its re-solve date forward.</p>
                    {p.hints.revealed.map((h, i) => (
                      <div className="hint" key={i}>
                        <div className="lab" style={{ color: "var(--vio)", marginBottom: 6 }}>
                          HINT {i + 1} · {h.kind.toUpperCase()}
                        </div>
                        <div style={{ color: "var(--fg)" }}>{h.text}</div>
                      </div>
                    ))}
                    {p.hints.locked.length > 0 && (
                      <div className="hint lock">
                        <div className="lab" style={{ marginBottom: 6 }}>
                          HINT {p.hints.revealed.length + 1} · {p.hints.locked[0]!.toUpperCase()}
                        </div>
                        <button style={{ color: "var(--vio)" }} disabled={hint.isPending} onClick={() => hint.mutate()}>
                          {hint.isPending ? "Revealing…" : `Reveal hint ${p.hints.revealed.length + 1} →`}
                        </button>
                      </div>
                    )}
                    {p.hints.locked.length > 1 && <p className="note">{p.hints.locked.length - 1} more after this one.</p>}
                  </div>
                )}

                {left === "solution" &&
                  (p.solution.unlocked ? (
                    <div className="stack" style={{ gap: 14 }}>
                      <div className="lab" style={{ color: p.attempt.assisted ? "var(--warn)" : "var(--grn)" }}>
                        REFERENCE SOLUTION · {p.attempt.assisted ? "ASSISTED" : "UNLOCKED"}
                      </div>
                      <button className="btn sm" style={{ alignSelf: "flex-start" }} onClick={() => setFile("solution")}>
                        {file === "solution" ? "Open in the editor ✓" : `Open the reference solution in the editor →`}
                      </button>
                      {p.solution.notes && (
                        <>
                          <p style={{ margin: 0 }} dangerouslySetInnerHTML={{ __html: marked.parseInline(p.solution.notes.explanation, { async: false }) }} />
                          <div className="kv" style={{ padding: "12px 0", borderTop: "1px solid var(--line2)", borderBottom: "1px solid var(--line2)" }}>
                            <span>time</span>
                            <code>{p.solution.notes.time}</code>
                            <span>space</span>
                            <code>{p.solution.notes.space}</code>
                          </div>
                        </>
                      )}
                    </div>
                  ) : (
                    <div className="stack" style={{ gap: 12, padding: 18, border: "1px dashed var(--line)", borderRadius: 4 }}>
                      <div className="lab">LOCKED · {latest ? `${latest.passed} OF ${latest.total} TESTS PASSING` : "NOT RUN YET"}</div>
                      <div style={{ color: "var(--fg)" }}>The solution opens when every test passes on Submit.</div>
                      <div style={{ fontSize: 12.5 }}>Reveal it now and the attempt is marked assisted, with a re-solve in 3 days.</div>
                      {confirm === "solution" ? (
                        <div style={{ display: "flex", gap: 8 }}>
                          <button className="btn sm" style={{ borderColor: "var(--warn)", color: "var(--warn)" }} onClick={() => solution.mutate()} disabled={solution.isPending}>
                            Reveal and mark assisted
                          </button>
                          <button className="btn sm" onClick={() => setConfirm(null)}>
                            Keep trying
                          </button>
                        </div>
                      ) : (
                        <button className="btn sm" style={{ alignSelf: "flex-start" }} onClick={() => setConfirm("solution")}>
                          Reveal anyway
                        </button>
                      )}
                    </div>
                  ))}

                {left === "related" && py && (
                  <div className="stack" style={{ gap: 10 }}>
                    <p className="note">
                      This practice problem is not a LeetCode problem. It opens when you log {p.unlocked_by.length > 1 ? "any of these" : "this one"}
                      {p.warmup ? ", and also while one of them is coming up next, so you can do it first as a warm-up" : ""}. It never schedules a review.
                    </p>
                    {p.unlocked_by.map((u) => (
                      <Link key={u.id} to="/d/$slug" params={{ slug: u.slug }} className="hint" style={{ display: "block", color: "var(--fg)" }}>
                        <span className="lab">LEETCODE</span>
                        <br />
                        <span style={{ fontSize: 14 }}>{u.title} ↗</span>
                      </Link>
                    ))}
                  </div>
                )}
                {left === "related" && !py && (
                  <div className="stack" style={{ gap: 10 }}>
                    {p.related.length === 0 && <p className="note">No related tracks listed.</p>}
                    {p.related.map((code) => (
                      <div key={code} className="hint">
                        <span className="lab">{code}</span>
                        <br />
                        <span style={{ fontSize: 14, color: "var(--fg)" }}>{SECTION_NAMES[code[0] as keyof typeof SECTION_NAMES] ?? code}</span>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            </section>
          ) : null}

          {/* ---------- centre: editor, actions, console ---------- */}
          <section className="pane c">
            <div className="ftabs">
              <div className="ftab-list">
                <button className={`ftab${file === "lib" ? " on" : ""}`} onClick={() => setFile("lib")}>
                  {py ? "solution.py" : "src/lib.rs"}
                  {code !== (latest?.code ?? p.starter) && <span className="dot" style={{ background: "var(--mut)" }} title="Changed since the last test run" />}
                </button>
                {!py && (
                  <button className={`ftab${file === "main" ? " on" : ""}`} onClick={() => setFile("main")} title="Scratch main: Run builds and runs it">
                    main.rs<small>SCRATCH</small>
                  </button>
                )}
                <button className={`ftab${file === "tests" ? " on" : ""}`} onClick={() => setFile("tests")}>
                  tests.{ext}
                  <small>READ-ONLY</small>
                </button>
                {p.hidden_tests && (
                  <button className={`ftab${file === "hidden" ? " on" : ""}`} onClick={() => setFile("hidden")} title="Unlocked by solving">
                    hidden.{ext}
                    <small>READ-ONLY</small>
                  </button>
                )}
                {p.solution.unlocked && p.solution.code && (
                  <button className={`ftab${file === "solution" ? " on" : ""}`} onClick={() => setFile("solution")} title="The reference solution">
                    {py ? "reference.py" : "solution.rs"}
                    <small>READ-ONLY</small>
                  </button>
                )}
              </div>
              <div className="tools">
                {/* The scratch tab runs main.rs; every other tab runs the tests against lib.rs. */}
                {file === "main" ? (
                  <button className="ebtn" onClick={doScratch} disabled={busy || p.status !== "ready"} title="Build main.rs with your lib.rs and run it (⌘')">
                    {scratch.isPending ? <span className="spin" aria-hidden="true" /> : <RunIcon kind="run" />}
                    {scratch.isPending ? "Running…" : "Run"}
                    <kbd>⌘'</kbd>
                  </button>
                ) : (
                  <button className="ebtn" onClick={doRun} disabled={busy || p.status !== "ready"} title="Run the visible tests (⌘↵)">
                    {run.isPending ? <span className="spin" aria-hidden="true" /> : <RunIcon kind="tests" />}
                    {run.isPending ? "Testing…" : "Run tests"}
                    <kbd>⌘↵</kbd>
                  </button>
                )}
                <EditorSettingsButton></EditorSettingsButton>
                {confirm === "reset" ? (
                  <span className="m" style={{ fontSize: 11, display: "flex", gap: 10 }}>
                    <button style={{ color: "var(--bad)" }} onClick={() => reset.mutate()}>
                      Reset to starter
                    </button>
                    <button style={{ color: "var(--dim)" }} onClick={() => setConfirm(null)}>
                      Cancel
                    </button>
                  </span>
                ) : (
                  <button className="m" style={{ fontSize: 11, color: "var(--dim)" }} onClick={() => setConfirm("reset")}>
                    Reset
                  </button>
                )}
              </div>
            </div>
            <div className="gbar">
              <div className="fp">
                <span style={{ color: "var(--fg)" }}>
                  {file === "lib" ? (py ? "solution.py" : "src/lib.rs") : file === "main" ? "src/bin/scratch.rs" : file === "solution" ? "reference solution" : file === "hidden" ? `tests/hidden.${ext}` : `tests/visible.${ext}`}
                </span>
                {py ? <span>Python 3 · standard library</span> : <span>edition 2021</span>}
                {!py && <span>{p.crates.length ? `crates: ${p.crates.join(" · ")}` : "std only"}</span>}
                <span>{file === "main" ? "⌘' runs it · tests don't run" : `${names.length} visible tests · ${hiddenCases ? `${hiddenCases.length} hidden, unlocked` : "hidden on Submit"}`}</span>
              </div>
              {lanesOn && file === "lib" && (
                <div className="lh">
                  <span>BORROW LANES</span>
                  <span style={{ color: lanes ? "var(--bad)" : "var(--grn)" }}>{lanes ? `${lanes.conflicts} CONFLICT${lanes.conflicts > 1 ? "S" : ""}` : "NO CONFLICTS"}</span>
                </div>
              )}
            </div>
            <div className="edit-host" hidden={file !== "lib"}>
              <Editor
                language={p.language}
                vim={editorSettings.vim}
                value={code}
                docKey={docKey}
                autocomplete={autocomplete}
                diagnostics={lensDiags}
                lanes={lanes}
                showLanes={lanesOn}
                lsp={raSession}
                lspEpoch={raEpoch}
                gotoKey="lib"
                onChange={setCode}
                onCursor={(l, c) => setCursor([l, c])}
                onRun={doRun}
                onSubmit={doSubmit}
                onScratch={doScratch}
                format={py ? undefined : api.format}
                formatOnPause={editorSettings.format_on_pause && !py}
                onFormatError={setFormatError}
                // ⌘S checks right away (with clippy when Live clippy is on) instead of after the 1 s pause.
                onSave={(c) => raSession?.save(c)}
              />
            </div>
            {!py && <div className="edit-host" hidden={file !== "main"}>
              <Editor
                vim={editorSettings.vim}
                value={main}
                docKey={`${p.id}:main`}
                autocomplete={autocomplete}
                diagnostics={scratchOut && lastAction === "scratch" ? scratchOut.diagnostics.map(forScratch) : []}
                gotoKey="main"
                onChange={setMain}
                onCursor={(l, c) => setCursor([l, c])}
                onRun={doRun}
                onSubmit={doSubmit}
                onScratch={doScratch}
                format={api.format}
                formatOnPause={editorSettings.format_on_pause}
                onFormatError={setFormatError}
              />
            </div>}
            {file === "tests" && (
              <div className="edit-host">
                <Editor language={p.language} value={p.visible_tests} docKey={`${p.id}:tests`} readOnly gotoKey="tests" />
              </div>
            )}
            {file === "hidden" && p.hidden_tests && (
              <div className="edit-host">
                <Editor language={p.language} value={p.hidden_tests} docKey={`${p.id}:hidden`} readOnly gotoKey="hidden" />
              </div>
            )}
            {file === "solution" && p.solution.code && (
              <div className="edit-host">
                <Editor language={p.language} value={p.solution.code} docKey={`${p.id}:solution`} readOnly />
              </div>
            )}
            <div className="statusb">
              {py ? (
                <span className="si" title="Tests run in the sandbox with Python 3. Syntax errors show when you run.">
                  <span className="dot" style={{ background: "var(--grn)" }} />
                  python 3 · sandboxed
                </span>
              ) : (
              <button className={`si${ra ? "" : " off"}`} onClick={() => setRa(!ra)} title={`${raDetail ? `${raDetail} · ` : ""}Click to turn rust-analyzer ${ra ? "off" : "on"}`} aria-pressed={ra}>
                <RaStatusLine status={raStatus} errors={raDiag.errors} warnings={raDiag.warnings} />
              </button>
              )}
              <button
                className={`si${autocomplete ? "" : " off"}`}
                onClick={() => setAutocomplete(!autocomplete)}
                title={`Autocomplete${autocomplete && !raSession && !py ? " (buffer words until rust-analyzer is ready)" : ""} · click to turn ${autocomplete ? "off" : "on"}`}
                aria-pressed={autocomplete}
              >
                <span className="dot" style={{ background: "var(--grn)" }} />
                autocomplete
              </button>
              {(borrowish || lanes) && (
                <button className={`si${lanesOn ? "" : " off"}`} onClick={() => setLanesOn(!lanesOn)} title={`Borrow lanes · click to turn ${lanesOn ? "off" : "on"}`} aria-pressed={lanesOn}>
                  <span className="dot" style={{ background: "var(--vio)" }} />
                  lanes
                </button>
              )}
              {formatError && (
                <span className="si sb-err" title={formatError}>
                  rustfmt: {formatError}
                </span>
              )}
              <span className="si sb-pos" title={py ? "Python 3 · Tab indents by 4 spaces · ⇧Tab dedents · sandboxed" : `rustc 1.98.1 stable · clippy ${liveClippy ? "while editing and " : ""}on test runs · ⌘S formats · sandboxed`}>
                Ln {cursor[0]}, Col {cursor[1]}
              </span>
            </div>
            <Console
              tab={consoleTab}
              setTab={(t) => showConsole(t)}
              open={consoleOpen}
              toggle={() => setConsoleOpen(!consoleOpen)}
              height={consoleH}
              setHeight={setConsoleH}
              summary={<ConsoleSummary busy={busy} scratchBusy={scratch.isPending} lastAction={lastAction} run={shown} scratch={scratchOut} />}
              errorCount={(lastAction === "scratch" ? scratchOut?.diagnostics : shown?.diagnostics)?.filter((d) => d.level === "error").length ?? 0}
            >
              {failure && (
                <p className="notice bad" style={{ margin: "0 0 12px" }}>
                  {failure instanceof ApiError ? failure.message : "The run failed to start. Is the API running?"}
                </p>
              )}
              {consoleTab === "compiler" &&
                (lastAction === "scratch" && scratchOut ? (
                  <DiagnosticsList diagnostics={scratchOut.diagnostics} busy={scratch.isPending} empty="main.rs and lib.rs compiled cleanly." onGoto={goto} />
                ) : (
                  <CompilerPanel run={shown} busy={run.isPending || submit.isPending} onGoto={goto} python={py} />
                ))}
              {consoleTab === "output" && <OutputPanel run={shown} scratch={scratchOut} scratchBusy={scratch.isPending} />}
              {consoleTab === "timeline" && (
                <div className="tline">
                  <div className="tline-runs">
                    {runs.length === 0 && <span className="note">No test runs yet. ⌘↵ runs the visible tests.</span>}
                    {[...runs].reverse().map((r) => {
                      const [label, color] = runLabel(r);
                      const on = shown?.id === r.id;
                      const squares = r.tests.length ? r.tests : names.map(() => null);
                      return (
                        <button key={r.id} className={`tline-run${on ? " on" : ""}`} style={{ borderColor: on ? color : undefined }} onClick={() => setSelected(r.id)}>
                          <span className="n">#{runs.indexOf(r) + 1}</span>
                          <span className="k">{r.kind === "submit" ? "submit" : "run"}</span>
                          <span className="sq">
                            {squares.slice(0, 16).map((t, i) => (
                              <i key={i} style={{ background: t === null ? "var(--line2)" : t.outcome === "passed" ? "var(--grn)" : "var(--bad)" }} />
                            ))}
                          </span>
                          <span className="l" style={{ color }}>
                            {label}
                          </span>
                          <span className="t">{new Date(r.created_at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}</span>
                        </button>
                      );
                    })}
                  </div>
                  <div className="tline-diff">
                    <div className="lab" style={{ letterSpacing: ".2em", marginBottom: 8 }}>
                      {shown && prevRun ? `DIFF · RUN #${shownIdx} → #${shownIdx + 1}` : "DIFF"}
                    </div>
                    {shown && prevRun ? (
                      (() => {
                        const lines = changedLines(prevRun.code, shown.code);
                        return lines.length === 0 ? (
                          <p className="note">No code changes between these runs.</p>
                        ) : (
                          <div className="diffx">
                            {lines.map((l, i) => (
                              <div key={i} className={l.op === "+" ? "add" : "del"}>
                                <span>{l.op === "+" ? "+" : "−"}</span>
                                {l.text}
                              </div>
                            ))}
                          </div>
                        );
                      })()
                    ) : (
                      <p className="note">Pick a run on the left; it's compared with the run before it.</p>
                    )}
                  </div>
                </div>
              )}
            </Console>
          </section>

          {rightOpen || narrow ? (
            <section className="pane r">
              <div className="rgrip" onPointerDown={(e) => dragWidth(e, rightW, setRightW, -1)} title="Drag to resize" aria-hidden="true" />
              <div className="tabs" style={{ padding: "0 18px" }} role="tablist">
                <button role="tab" className="on" aria-selected="true">
                  {shown && shown.tests.length ? `Tests ${shown.passed}/${shown.total}` : "Tests"}
                </button>
              </div>
              <div className="pbody" style={{ padding: 18 }}>
                {p.attempt.solved && (
                  <div className="solved">
                    <span className="lab" style={{ color: "var(--grn)" }}>
                      SOLVED{p.attempt.assisted ? " · ASSISTED" : ""}
                    </span>
                    {p.next ? (
                      <Link to="/p/$id" params={{ id: p.next }}>
                        Next problem →
                      </Link>
                    ) : (
                      <Link to="/t/$track" params={{ track: p.track.slug }}>
                        Back to {p.track.name} →
                      </Link>
                    )}
                  </div>
                )}
                <TestsPanel python={py} run={shown} cases={cases} hiddenCases={hiddenCases} busy={run.isPending || submit.isPending} open={open} setOpen={setOpen} runNo={shownIdx + 1} onRun={doRun} canRun={p.status === "ready"} />
              </div>
              <FocusTimer />
              <div className="acts">
                <button className="go" onClick={doSubmit} disabled={busy || p.status !== "ready"} title="Run the visible and hidden tests (⇧⌘↵)">
                  {submit.isPending && <span className="spin" aria-hidden="true" />}
                  {submit.isPending ? "Submitting…" : "Submit"}
                </button>
              </div>
            </section>
          ) : null}
        </div>
      </main>
    </>
  );
}

function RaStatusLine({ status, detail, errors, warnings }: { status: RaStatus; detail?: string; errors: number; warnings: number }) {
  const [text, color] =
    status === "off"
      ? ["rust-analyzer off · errors on run", "var(--line)"]
      : status === "connecting"
        ? ["rust-analyzer · starting…", "var(--dim)"]
        : status === "indexing"
          ? ["rust-analyzer · indexing…", "var(--acc)"]
          : status === "error"
            ? [`rust-analyzer unavailable${detail ? ` · ${detail}` : ""}`, "var(--bad)"]
            : [
                `rust-analyzer · ${errors} error${errors === 1 ? "" : "s"}${warnings ? ` · ${warnings} warning${warnings === 1 ? "" : "s"}` : ""}`,
                errors ? "var(--bad)" : "var(--grn)",
              ];
  return (
    <span style={{ display: "flex", alignItems: "center", gap: 6 }} title={detail}>
      <span className="dot" style={{ background: color }} />
      {text}
    </span>
  );
}

function Rule({ ok, children }: { ok: boolean; children: React.ReactNode }) {
  return (
    <>
      <span style={{ color: ok ? "var(--grn)" : "var(--bad)" }}>{ok ? "+" : "×"}</span>
      <span>{children}</span>
    </>
  );
}

function RunIcon({ kind }: { kind: "run" | "tests" }) {
  return kind === "run" ? (
    <svg viewBox="0 0 12 12" aria-hidden="true">
      <path d="M3.2 1.9v8.2a.7.7 0 0 0 1.05.6l6.6-4.1a.7.7 0 0 0 0-1.2L4.25 1.3a.7.7 0 0 0-1.05.6z" fill="currentColor" />
    </svg>
  ) : (
    <svg viewBox="0 0 12 12" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round">
      <path d="M4.3 1.2h3.4M4.9 1.2v3.3L1.9 9.7a.9.9 0 0 0 .8 1.3h6.6a.9.9 0 0 0 .8-1.3L7.1 4.5V1.2" />
      <path d="M3.1 7.6h5.8" />
    </svg>
  );
}

type Region = "left" | "bottom" | "right";

const REGIONS: [Region, string, string][] = [
  ["left", "problem panel", "⌘B"],
  ["bottom", "console", "⌘J"],
  ["right", "tests panel", "⌥⌘B"],
];

/** VS Code-style toggles: each icon hides or shows one region of the workspace. */
function LayoutButtons({ left, bottom, right, toggle }: { left: boolean; bottom: boolean; right: boolean; toggle: (r: Region) => void }) {
  const shown = { left, bottom, right };
  return (
    <span className="lay" role="group" aria-label="Layout">
      {REGIONS.map(([r, name, keys]) => {
        const label = `${shown[r] ? "Hide" : "Show"} the ${name} · ${keys}`;
        return (
          <button key={r} className={shown[r] ? "on" : ""} onClick={() => toggle(r)} title={label} aria-label={label} aria-pressed={shown[r]}>
            <svg viewBox="0 0 15 13" aria-hidden="true">
              <rect x="0.75" y="0.75" width="13.5" height="11.5" rx="2" fill="none" stroke="currentColor" strokeWidth="1.3" />
              {r === "left" ? (
                <rect className="fill" x="1.5" y="1.5" width="4.5" height="10" rx="0.8" />
              ) : r === "right" ? (
                <rect className="fill" x="9" y="1.5" width="4.5" height="10" rx="0.8" />
              ) : (
                <rect className="fill" x="1.5" y="7.5" width="12" height="4" rx="0.8" />
              )}
            </svg>
          </button>
        );
      })}
    </span>
  );
}

function CaseRows({ c }: { c: TestCase }) {
  if (c.input === undefined) return null;
  return (
    <div className="tcase-kv">
      <span>input</span>
      <code>{c.input}</code>
      <span>expected</span>
      <code style={{ color: "var(--grn)" }}>{c.expected}</code>
    </div>
  );
}

/** A case that ran and did not pass: these are listed first. */
function failedCase(t: TestOutcome | undefined): boolean {
  return !!t && t.outcome !== "passed";
}

/** Tick, cross or dashed ring: the outcome without relying on colour. */
function CaseIcon({ t, waiting }: { t: TestOutcome | undefined; waiting: boolean }) {
  const ok = t?.outcome === "passed";
  const colour = !t ? (waiting ? "var(--acc)" : "var(--dim)") : ok ? "var(--grn)" : "var(--bad)";
  return (
    <svg className="cicon" viewBox="0 0 16 16" width="13" height="13" style={{ color: colour }} aria-hidden="true">
      {!t ? (
        <circle cx="8" cy="8" r="5" fill="none" stroke="currentColor" strokeWidth="1.6" strokeDasharray="3 3" />
      ) : ok ? (
        <path d="M3 8.5l3.2 3.2L13 4.8" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
      ) : (
        <path d="M4 4l8 8M12 4l-8 8" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
      )}
    </svg>
  );
}

function CaseCard({
  python,
  c,
  id,
  t,
  run,
  hidden = false,
  busy = false,
  open,
  setOpen,
}: {
  python?: boolean;
  c: TestCase;
  id: string;
  t: TestOutcome | undefined;
  run: RunView | null;
  hidden?: boolean;
  busy?: boolean;
  open: string | null;
  setOpen: (n: string | null) => void;
}) {
  const ok = t?.outcome === "passed";
  const failed = t && !ok;
  const expanded = open === id || (failed && open === null);
  return (
    <div className={`tcase${failed ? " bad" : ""}`}>
      <button className="tcase-h" onClick={() => setOpen(expanded ? "" : id)} aria-expanded={expanded}>
        <CaseIcon t={t} waiting={busy} />
        <span className="nm">
          {hidden && <span style={{ color: "var(--mut)" }}>hidden · </span>}
          {c.name}
        </span>
        <span className="tval">
          {!t
            ? run
              ? run.status === "compile_error"
                ? "not compiled"
                : run.status === "timeout"
                  ? "timed out"
                  : ""
              : ""
            : t.outcome === "timed_out"
              ? "timed out"
              : t.duration_ms !== null
                ? `${t.duration_ms.toFixed(1)} ms`
                : ok
                  ? "passed"
                  : "failed"}
        </span>
        <span className="chev">{expanded ? "▾" : "▸"}</span>
      </button>
      <CaseRows c={c} />
      {expanded && (
        <div className="tcase-more">
          {c.setup && (
            <>
              <span>setup</span>
              <pre>{c.setup}</pre>
            </>
          )}
          {c.call && (
            <>
              <span>call</span>
              <pre>{c.call}</pre>
            </>
          )}
          {t?.check && (
            <>
              <span>got</span>
              <pre style={{ color: "var(--bad)" }}>{t.check.got}</pre>
            </>
          )}
          {t && !t.check && t.panic && (
            <>
              <span>panic</span>
              <pre style={{ color: "var(--bad)" }}>{t.panic}</pre>
            </>
          )}
          {t?.stdout && (
            <>
              <span>stdout</span>
              <pre>{t.stdout}</pre>
            </>
          )}
          {c.input === undefined && !t && (
            <span className="note" style={{ gridColumn: "1 / -1" }}>
              This test doesn't use {python ? "check()" : "check!"}; see {hidden ? "hidden" : "tests"}.{python ? "py" : "rs"}.
            </span>
          )}
        </div>
      )}
    </div>
  );
}

function TestsPanel({
  python,
  run,
  cases,
  hiddenCases,
  busy,
  open,
  setOpen,
  runNo,
  onRun,
  canRun,
}: {
  python?: boolean;
  run: RunView | null;
  onRun: () => void;
  canRun: boolean;
  cases: TestCase[];
  /** Parsed hidden tests, once the problem has been solved. */
  hiddenCases: TestCase[] | null;
  busy: boolean;
  open: string | null;
  setOpen: (n: string | null) => void;
  runNo: number;
}) {
  const byName = new Map(run?.tests.map((t) => [t.suite + t.name, t]) ?? []);
  const visible = run?.tests.filter((t) => t.suite === "visible") ?? [];
  const hidden = run?.tests.filter((t) => t.suite === "hidden") ?? [];
  const firstError = run?.diagnostics.find((d) => d.level === "error");
  const [dismissed, setDismissed] = useState<number | null>(null);
  // A shadow under the strip once the list has scrolled beneath it.
  const stripRef = useRef<HTMLDivElement>(null);
  const [stuck, setStuck] = useState(false);
  useEffect(() => {
    const box = stripRef.current?.closest(".pbody");
    if (!box) return;
    const on = () => setStuck(box.scrollTop > 4);
    on();
    box.addEventListener("scroll", on, { passive: true });
    return () => box.removeEventListener("scroll", on);
  }, []);
  const status = busy
    ? (["Running…", "var(--acc)"] as const)
    : !run
      ? (["Not run yet", "var(--dim)"] as const)
      : run.status === "compile_error"
        ? (["Doesn't compile", "var(--bad)"] as const)
        : run.status === "timeout"
          ? (["Timed out", "var(--bad)"] as const)
          : ([`${run.passed} / ${run.total} passing`, run.status === "passed" ? "var(--grn)" : "var(--fg)"] as const);
  return (
    <div className="stack" style={{ gap: 14 }}>
      <div className={`tstrip${stuck ? " stuck" : ""}`} ref={stripRef}>
        <div className="tsum">
          <b style={{ color: status[1] }}>{status[0]}</b>
          <span>{run && !busy ? `${run.kind} #${runNo} · ${(run.duration_ms / 1000).toFixed(1)}s` : `${cases.length} visible · ${hiddenCases ? `${hiddenCases.length} hidden` : "hidden on Submit"}`}</span>
          <button className="ebtn tstrip-run" onClick={onRun} disabled={busy || !canRun} title="Run the visible tests (⌘↵)">
            {busy ? <span className="spin" aria-hidden="true" /> : null}
            {busy ? "Testing…" : "Run tests"}
          </button>
        </div>
        <div className="passbar" aria-hidden="true">
          {(run && !busy && run.tests.length ? run.tests : [...cases, ...(hiddenCases ?? [])].map(() => null)).map((t, i) => (
            <span key={i} className={busy ? "wait" : undefined} style={{ background: busy ? undefined : !t ? "var(--line2)" : t.outcome === "passed" ? "var(--grn)" : "var(--bad)" }} />
          ))}
        </div>
      </div>
      {run && !busy && run.tests.length > 0 && (
        <div className="tcounts">
          <span>
            visible{" "}
            <b style={{ color: visible.every((t) => t.outcome === "passed") ? "var(--grn)" : "var(--bad)" }}>
              {visible.filter((t) => t.outcome === "passed").length}/{visible.length}
            </b>
          </span>
          {hidden.length > 0 && (
            <span>
              hidden{" "}
              <b style={{ color: hidden.every((t) => t.outcome === "passed") ? "var(--grn)" : "var(--bad)" }}>
                {hidden.filter((t) => t.outcome === "passed").length}/{hidden.length}
              </b>
            </span>
          )}
          {!python && <span>clippy {run.diagnostics.filter((d) => d.level === "warning").length} warnings</span>}
        </div>
      )}
      {run?.violations.length ? (
        <div className="rulebox">
          <span className="lab" style={{ color: "var(--warn)" }}>
            RULE BROKEN · {run.status === "passed" ? "TESTS PASS, BUT IT DOESN'T COUNT" : "FIX THE RULES TOO"}
          </span>
          {run.violations.map((v, i) => (
            <div key={i}>
              {v.line !== null && <span className="m">line {v.line} · </span>}
              {v.message}
            </div>
          ))}
        </div>
      ) : null}
      {firstError && !busy && dismissed !== run?.id && (
        <div className="errbox">
          <button className="box-x" onClick={() => setDismissed(run?.id ?? null)} title="Hide until the next run" aria-label="Hide this error">
            ×
          </button>
          <span className="lab" style={{ color: "var(--bad)" }}>
            {firstError.code ?? "ERROR"} · LINE {firstError.spans.find((s) => s.primary)?.line_start ?? "?"}
          </span>
          <br />
          {firstError.message}
          <div className="note" style={{ marginTop: 6 }}>
            Full output in the Compiler tab below.
          </div>
        </div>
      )}
      <div className="tcases">
        {[...cases]
          .sort((x, y) => Number(failedCase(byName.get("visible" + y.name))) - Number(failedCase(byName.get("visible" + x.name))))
          .map((c) => (
          <CaseCard python={python} busy={busy} key={c.name} c={c} id={c.name} t={busy ? undefined : byName.get("visible" + c.name)} run={busy ? null : run} open={open} setOpen={setOpen} />
        ))}
        {hiddenCases
          ? hiddenCases.map((c) => (
              <CaseCard python={python} busy={busy} key={"h" + c.name} c={c} id={"hidden:" + c.name} hidden t={busy ? undefined : byName.get("hidden" + c.name)} run={busy ? null : run} open={open} setOpen={setOpen} />
            ))
          : !busy &&
            hidden.map((t) => (
              <div key={t.name} className={`tcase${t.outcome !== "passed" ? " bad" : ""}`}>
                <div className="tcase-h">
                  <span className="sqr" style={{ background: t.outcome === "passed" ? "var(--grn)" : "var(--bad)" }} />
                  <span className="nm" style={{ color: "var(--mut)" }}>
                    hidden · {t.name}
                  </span>
                  <span className="tval">{t.outcome === "passed" ? "passed" : t.outcome === "timed_out" ? "timed out" : "failed · input withheld"}</span>
                </div>
              </div>
            ))}
      </div>
    </div>
  );
}

type Goto = (path: string, line: number, col: number) => void;

function CompilerPanel({ run, busy, onGoto, python }: { run: RunView | null; busy: boolean; onGoto: Goto; python?: boolean }) {
  if (busy) return <Pending text={python ? "Running…" : "Compiling…"} />;
  if (!run) return <p className="note">{python ? "Syntax and indentation errors show up here after a run." : "Compiler errors and clippy lints show up here after a run."}</p>;
  return <DiagnosticsList diagnostics={run.diagnostics} busy={false} empty={python ? "No syntax errors." : "Compiled cleanly. clippy found nothing."} onGoto={onGoto} />;
}

function Pending({ text }: { text: string }) {
  return (
    <div className="pending">
      <span className="spin" aria-hidden="true" />
      {text}
    </div>
  );
}

/** Where the docs for a diagnostic live: rustc's error index or clippy's lint list. */
function docsFor(code: string | null): string | null {
  if (!code) return null;
  if (/^E\d{4}$/.test(code)) return `https://doc.rust-lang.org/error_codes/${code}.html`;
  if (code.startsWith("clippy::")) return `https://rust-lang.github.io/rust-clippy/master/index.html#${code.slice(8)}`;
  return null;
}

function DiagCard({ d, onGoto }: { d: Diagnostic; onGoto: Goto }) {
  const primary = d.spans.find((s) => s.primary) ?? d.spans[0];
  const docs = docsFor(d.code);
  // rustc's first line repeats the header; keep the snippet, labels and help below it.
  const body = d.rendered.split("\n").slice(1).join("\n").trimEnd();
  return (
    <div className={`dcard ${d.level}`}>
      <div className="dcard-h">
        <span className={`sev ${d.level}`}>{d.level}</span>
        {d.code &&
          (docs ? (
            <a className="dcode" href={docs} target="_blank" rel="noreferrer" title="Open the explanation">
              {d.code}
            </a>
          ) : (
            <span className="dcode">{d.code}</span>
          ))}
        <span className="dmsg">{d.message}</span>
        {primary && (
          <button className="dloc" onClick={() => onGoto(primary.file, primary.line_start, primary.col_start)} title="Jump to this line">
            {primary.file}:{primary.line_start}:{primary.col_start}
          </button>
        )}
      </div>
      {body && (
        <pre className="dcard-b">
          <Ansi text={body} />
        </pre>
      )}
    </div>
  );
}

function DiagnosticsList({ diagnostics, busy, empty, onGoto }: { diagnostics: Diagnostic[]; busy: boolean; empty: string; onGoto: Goto }) {
  const errors = diagnostics.filter((d) => d.level === "error");
  const warnings = diagnostics.filter((d) => d.level !== "error");
  const [showWarnings, setShowWarnings] = useState(errors.length === 0);
  if (busy) return <Pending text="Compiling…" />;
  if (diagnostics.length === 0)
    return (
      <div className="diag-ok">
        <span className="chip2 ok">✓ clean</span>
        {empty}
      </div>
    );
  return (
    <div className="diag">
      <div className="diag-sum">
        <span className={`chip2 ${errors.length ? "bad" : "ok"}`}>
          {errors.length} error{errors.length === 1 ? "" : "s"}
        </span>
        <span className={`chip2 ${warnings.length ? "warn" : "ok"}`}>
          {warnings.length} warning{warnings.length === 1 ? "" : "s"}
        </span>
        {errors.length > 0 && warnings.length > 0 && (
          <button className="diag-toggle" onClick={() => setShowWarnings(!showWarnings)}>
            {showWarnings ? "hide warnings" : "show warnings"}
          </button>
        )}
      </div>
      {errors.map((d, i) => (
        <DiagCard key={`e${i}`} d={d} onGoto={onGoto} />
      ))}
      {showWarnings && warnings.map((d, i) => <DiagCard key={`w${i}`} d={d} onGoto={onGoto} />)}
    </div>
  );
}

/** One output line: `dbg!` lines split into location, expression and value; panics highlighted. */
function OutLine({ line, stderr }: { line: string; stderr?: boolean }) {
  const dbg = /^\[(.+?:\d+:\d+)\] (.+?) = (.*)$/.exec(line);
  if (dbg)
    return (
      <span className="ln">
        <span className="dbg-loc">[{dbg[1]}]</span> <span className="dbg-expr">{dbg[2]}</span> <span className="dbg-eq">=</span> <span className="dbg-val">{dbg[3]}</span>
      </span>
    );
  if (/^thread '.*' panicked at /.test(line)) return <span className="ln panic">{line}</span>;
  if (/^note: run with `RUST_BACKTRACE=1`/.test(line)) return <span className="ln dimln">{line}</span>;
  return (
    <span className={`ln${stderr ? " err" : ""}`}>
      <Ansi text={line} />
    </span>
  );
}

function CopyButton({ text }: { text: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <button
      className="copy"
      onClick={() =>
        navigator.clipboard.writeText(text).then(
          () => {
            setCopied(true);
            setTimeout(() => setCopied(false), 1200);
          },
          () => undefined,
        )
      }
    >
      {copied ? "copied" : "copy"}
    </button>
  );
}

function Terminal({
  head,
  badge,
  badgeKind,
  meta,
  stdout,
  stderr,
}: {
  head: React.ReactNode;
  badge?: string;
  badgeKind?: "ok" | "bad" | "warn";
  meta?: string;
  stdout: string;
  stderr?: string;
}) {
  const lines = (t: string) => t.replace(/\n$/, "").split("\n");
  const all = [stdout, stderr].filter(Boolean).join("\n");
  return (
    <div className="term">
      <div className="term-h">
        <span className="cmd">{head}</span>
        {badge && <span className={`badge ${badgeKind ?? "ok"}`}>{badge}</span>}
        {meta && <span className="meta">{meta}</span>}
        {all && <CopyButton text={all} />}
      </div>
      <pre className="term-b">
        {stdout ? lines(stdout).map((l, i) => <OutLine key={`o${i}`} line={l} />) : !stderr && <span className="ln dimln">(no output)</span>}
        {stderr && (
          <>
            {stdout && <span className="ln sep">── stderr ──</span>}
            {lines(stderr).map((l, i) => (
              <OutLine key={`e${i}`} line={l} stderr />
            ))}
          </>
        )}
      </pre>
    </div>
  );
}

function OutputPanel({ run, scratch, scratchBusy }: { run: RunView | null; scratch: ScratchResult | null; scratchBusy: boolean }) {
  const printed = run?.tests.filter((t) => t.stdout) ?? [];
  const kind = (s: ScratchResult) => (s.status === "ok" ? "ok" : s.status === "timeout" ? "warn" : "bad");
  return (
    <div className="outp">
      {scratchBusy ? (
        <Pending text="Building and running main.rs…" />
      ) : !scratch ? (
        <div className="term">
          <div className="term-h">
            <span className="cmd">
              <b>$</b> cargo run --bin scratch
            </span>
          </div>
          <pre className="term-b">
            <span className="ln dimln">Write a main in main.rs and press ▷ Run (⌘') to see its output here.</span>
          </pre>
        </div>
      ) : scratch.status === "compile_error" ? (
        <div className="diag-ok">
          <span className="chip2 bad">didn't compile</span>
          main.rs or lib.rs has errors: see the Compiler tab.
        </div>
      ) : (
        <Terminal
          head={
            <>
              <b>$</b> cargo run --bin scratch
            </>
          }
          badge={scratchStatus(scratch)}
          badgeKind={kind(scratch)}
          meta={`${(scratch.duration_ms / 1000).toFixed(1)}s`}
          stdout={scratch.stdout}
          stderr={scratch.stderr}
        />
      )}
      <div className="lab" style={{ margin: "18px 0 8px" }}>
        PRINTED BY TESTS
      </div>
      {printed.length === 0 ? (
        <p className="note">println!, eprintln! and dbg! output from the visible tests shows up here after a test run, grouped by test. Hidden tests' output stays hidden.</p>
      ) : (
        printed.map((t) => (
          <Terminal
            key={t.suite + t.name}
            head={
              <span style={{ color: t.outcome === "passed" ? "var(--grn)" : "var(--bad)" }}>
                {t.outcome === "passed" ? "✓" : "✗"} {t.name}
              </span>
            }
            meta={t.duration_ms !== null ? `${t.duration_ms.toFixed(1)} ms` : undefined}
            stdout={t.stdout}
          />
        ))
      )}
    </div>
  );
}

const scratchStatus = (s: ScratchResult) =>
  s.status === "ok" ? "exit 0" : s.status === "exited" ? `exit ${s.exit_code ?? "?"}` : s.status === "timeout" ? "timed out" : "compile error";

/** Diagnostics from a scratch build point at src/bin/scratch.rs; the main.rs editor shows those only. */
function forScratch(d: Diagnostic): Diagnostic {
  return { ...d, spans: d.spans.filter((s) => s.file === "src/bin/scratch.rs").map((s) => ({ ...s, file: "src/lib.rs" })) };
}

function ConsoleSummary({
  busy,
  scratchBusy,
  lastAction,
  run,
  scratch,
}: {
  busy: boolean;
  scratchBusy: boolean;
  lastAction: "tests" | "scratch";
  run: RunView | null;
  scratch: ScratchResult | null;
}) {
  if (busy) return <span style={{ color: "var(--acc)" }}>{scratchBusy ? "running main.rs…" : "running tests…"}</span>;
  if (lastAction === "scratch" && scratch) {
    const ok = scratch.status === "ok";
    return <span style={{ color: ok ? "var(--grn)" : "var(--bad)" }}>main.rs · {scratchStatus(scratch)}</span>;
  }
  if (!run) return <span>no runs yet</span>;
  const [label, color] = runLabel(run);
  return (
    <span style={{ color }}>
      {run.kind} · {label}
    </span>
  );
}

function Console({
  tab,
  setTab,
  open,
  toggle,
  height,
  setHeight,
  summary,
  errorCount,
  children,
}: {
  tab: ConsoleTab;
  setTab: (t: ConsoleTab) => void;
  open: boolean;
  toggle: () => void;
  height: number;
  setHeight: (h: number) => void;
  summary: React.ReactNode;
  errorCount: number;
  children: React.ReactNode;
}) {
  const drag = (e: React.PointerEvent<HTMLDivElement>) => {
    if (!open) return;
    const startY = e.clientY;
    const startH = height;
    const pane = e.currentTarget.closest(".pane") as HTMLElement | null;
    const max = Math.max(CONSOLE_MIN, (pane?.clientHeight ?? 800) - 180);
    const move = (ev: PointerEvent) => setHeight(Math.min(max, Math.max(CONSOLE_MIN, startH + startY - ev.clientY)));
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      document.body.style.cursor = "";
    };
    document.body.style.cursor = "row-resize";
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };
  if (!open) return null;
  return (
    <div className="console" style={{ height }}>
      <div className="console-grip" onPointerDown={drag} aria-hidden="true" />
      <div className="console-bar">
        <div className="console-tabs" role="tablist">
          {(
            [
              ["compiler", errorCount ? `Compiler · ${errorCount}` : "Compiler"],
              ["output", "Output"],
              ["timeline", "Timeline"],
            ] as const
          ).map(([k, label]) => (
            <button
              key={k}
              role="tab"
              className={open && tab === k ? "on" : ""}
              aria-selected={open && tab === k}
              onClick={() => (open && tab === k ? toggle() : setTab(k))}
              style={k === "compiler" && errorCount ? { color: "var(--bad)" } : undefined}
            >
              {label}
            </button>
          ))}
        </div>
        <div className="console-sum">{summary}</div>
        <button className="console-toggle" onClick={toggle} title="Hide the console until the next run">
          ▾
        </button>
      </div>
      <div className="console-body">{children}</div>
    </div>
  );
}

/** Drags a side panel's inner edge (`dir` 1 for the left panel, -1 for the right); kept between 260 px and 45–60 % of the window. */
function dragWidth(e: React.PointerEvent, start: number, set: (w: number) => void, dir: 1 | -1) {
  e.preventDefault();
  const x0 = e.clientX;
  const max = window.innerWidth * (dir === 1 ? 0.6 : 0.45);
  const move = (ev: PointerEvent) => set(Math.min(max, Math.max(260, start + dir * (ev.clientX - x0))));
  const up = () => {
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", up);
    document.body.style.cursor = "";
    document.body.style.userSelect = "";
  };
  document.body.style.cursor = "col-resize";
  document.body.style.userSelect = "none";
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up);
}

function useStoredNumber(key: string, initial: number): [number, (n: number) => void] {
  const [v, set] = useState(() => {
    try {
      const n = Number(localStorage.getItem(key));
      return Number.isFinite(n) && n > 0 ? n : initial;
    } catch {
      return initial;
    }
  });
  return [
    v,
    (n) => {
      set(n);
      try {
        localStorage.setItem(key, String(Math.round(n)));
      } catch {
        // Storage can be unavailable; the size still applies for this visit.
      }
    },
  ];
}

/** Reports active editing time: every 30 s while the tab is visible and you've typed or clicked in the last minute. */
function useFocusHeartbeat(id: string) {
  useEffect(() => {
    let last = Date.now();
    let active = Date.now();
    const mark = () => {
      active = Date.now();
    };
    window.addEventListener("keydown", mark);
    window.addEventListener("mousedown", mark);
    const timer = setInterval(() => {
      const now = Date.now();
      const seconds = Math.round((now - last) / 1000);
      last = now;
      if (document.visibilityState === "visible" && now - active < 60_000) api.focus(id, Math.min(seconds, 120)).catch(() => undefined);
    }, 30_000);
    return () => {
      clearInterval(timer);
      window.removeEventListener("keydown", mark);
      window.removeEventListener("mousedown", mark);
    };
  }, [id]);
}
