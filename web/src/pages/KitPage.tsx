// The component kit (docs/mockups/course-motion.html, screen 3): every button, slider, bar and popup the app is built from, live.

import { useEffect, useRef, useState, type CSSProperties } from "react";
import { Btn as UiBtn, ConfirmButton, Segmented, Switch } from "../components/ui";
import { GradeGlyph } from "../components/icons";
import { Header } from "../components/Header";
import { Celebration, Modal } from "../components/kit";
import { MockRoot, reducedMotion, rise, useReady } from "../components/mock";
import { toast } from "../components/toasts";

const Btn = ({ cls = "", children, ...rest }: { cls?: string; children: React.ReactNode } & React.ButtonHTMLAttributes<HTMLButtonElement>) => (
    <button className={`k-cta ${cls} k-fx`.replace(/\b(\w[\w-]*)\b/g, (m) => (m.startsWith("k-") ? m : `k-${m}`)).replace("k-k-", "k-")} {...rest}>
        <span className="k-lbl">{children}</span>
    </button>
);

const Sw = ({ on, onClick, label }: { on: boolean; onClick: () => void; label: string }) => <button className={`k-sw${on ? " k-on" : ""}`} role="switch" aria-checked={on} aria-label={label} onClick={onClick} />;

function Range({ label, min, max, step = 1, init, unit }: { label: string; min: number; max: number; step?: number; init: number; unit: string }) {
    const [v, setV] = useState(init);
    return (
        <label className="k-rng">
            <span className="k-h">
                <span>{label}</span>
                <b>
                    {v} {unit}
                </b>
            </span>
            <input type="range" min={min} max={max} step={step} value={v} style={{ "--v": `${(100 * (v - min)) / (max - min)}%` } as CSSProperties} onChange={(e) => setV(+e.target.value)} />
        </label>
    );
}

const WG = 5;

export function KitPage() {
    const ready = useReady();
    const [on, setOn] = useState(false);
    const [grp, setGrp] = useState("All");
    const [n, setN] = useState(3);
    const [step, setStep] = useState(2);
    const [secs, setSecs] = useState(45 * 60);
    const [ld, setLd] = useState<"" | "busy" | "done">("");
    const [cp, setCp] = useState(false);
    const [hold, setHold] = useState(false);
    const holdTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
    const [ws, setWs] = useState<{ cells: ("w" | "p" | "f" | "")[]; big: ReactLabel; meta: string; ok: boolean; running: boolean; fail: boolean }>({ cells: Array(WG).fill(""), big: "Not run yet", meta: "5 visible · hidden on Submit", ok: false, running: false, fail: false });
    const [sub, setSub] = useState<"" | "busy" | "done" | "shake">("");
    const [dlg, setDlg] = useState<"" | "win" | "lose" | "sheet">("");
    const [pop, setPop] = useState(false);
    useEffect(() => {
        const t = setInterval(() => setSecs((s) => (s <= 0 ? 45 * 60 : s - 1)), 1000);
        return () => clearInterval(t);
    }, []);
    useEffect(() => {
        if (!pop) return;
        const close = () => setPop(false);
        document.addEventListener("click", close);
        return () => document.removeEventListener("click", close);
    }, [pop]);
    const fill = (w: number): CSSProperties => ({ width: ready ? `${w}%` : 0 });
    const step0 = reducedMotion() ? 0 : 240;
    const runWs = (after?: (fail: boolean) => void) => {
        const fail = ws.fail;
        setWs((s) => ({ ...s, running: true, big: "Running…", meta: "", cells: Array(WG).fill("w") }));
        let i = 0;
        const go = () => {
            if (i < WG) {
                const k = i++;
                setWs((s) => ({ ...s, cells: s.cells.map((c, j) => (j === k ? (fail && k % 2 === 1 ? "f" : "p") : c)) }));
                setTimeout(go, step0);
            } else {
                setWs((s) => ({ ...s, running: false, ok: !fail, big: fail ? "2 of 5 failed" : "5 of 5 passed", meta: "just now · 0.8s" }));
                after?.(fail);
            }
        };
        setTimeout(go, reducedMotion() ? 0 : 400);
    };
    const submit = () => {
        if (!ws.ok) {
            setSub("shake");
            setTimeout(() => setSub(""), 500);
            toast(ws.fail ? "er" : "in", ws.fail ? "Submit blocked" : "Run the tests first", ws.fail ? "2 visible tests fail." : "Submit needs a passing run.");
            return;
        }
        setSub("busy");
        setTimeout(() => {
            setSub("done");
            setTimeout(() => setSub(""), 1600);
            setDlg("win");
        }, reducedMotion() ? 0 : 900);
    };
    const mm = `${String(Math.floor(secs / 60)).padStart(2, "0")}:${String(secs % 60).padStart(2, "0")}`;
    return (
        <>
            <Header />
            <MockRoot>
                <div className="k-wrap k-kit">
                    <div>
                        <div className="k-eye">
                            <b>COMPONENTS</b> / BUTTONS, SLIDERS, BARS
                        </div>
                        <h1 style={{ marginTop: 8 }}>Every control, one kit.</h1>
                        <p className="k-lead">Press, hover and drag everything. Each button gives feedback on press (ripple, depth, squish or glow); bars move, sliders fill as you drag.</p>
                    </div>

                    <div className="k-kc k-rv" style={rise(0).style}>
                        <h3>Buttons: sizes</h3>
                        <p className="k-sub">Five sizes for one style. XL is for the single action of a screen.</p>
                        <div className="k-rowf">
                            <button className="k-cta k-xl k-fx"><span className="k-lbl">Start the course</span><span className="k-ar">→</span></button>
                            <button className="k-cta k-lg k-fx"><span className="k-lbl">Resume stage</span><span className="k-ar">→</span></button>
                            <Btn>Run tests</Btn>
                            <Btn cls="sm">Copy command</Btn>
                            <Btn cls="xs">Hint</Btn>
                        </div>
                    </div>

                    <div className="k-kc k-rv" style={rise(1).style}>
                        <h3>Buttons: shapes and variants</h3>
                        <p className="k-sub">Rectangle, rounded, pill, square, icon, circle and floating; filled, tonal, outline, secondary, danger and link.</p>
                        <div className="k-rowf">
                            <small>SHAPES</small>
                            <Btn cls="sq">Square</Btn>
                            <Btn>Rounded</Btn>
                            <Btn cls="pill">Pill</Btn>
                            <button className="k-cta k-icon k-fx" aria-label="Run">▶</button>
                            <button className="k-cta k-circle k-fx" aria-label="Add">＋</button>
                            <button className="k-cta k-fab k-fx" aria-label="Floating action">✦</button>
                        </div>
                        <div className="k-rowf">
                            <small>VARIANTS</small>
                            <Btn>Filled</Btn>
                            <Btn cls="tonal">Tonal</Btn>
                            <Btn cls="out">Outline</Btn>
                            <Btn cls="sec">Secondary</Btn>
                            <Btn cls="danger">Reset stage</Btn>
                            <Btn cls="link">Link style</Btn>
                            <button className="k-cta" disabled><span className="k-lbl">Disabled</span></button>
                        </div>
                        <div className="k-rowf">
                            <small>SPLIT, GROUP, KEYS</small>
                            <span className="k-split">
                                <Btn>Run tests</Btn>
                                <button className="k-cta k-fx" aria-label="More">▾</button>
                            </span>
                            <span className="k-grp">
                                {["All", "Fix this", "Write it"].map((g) => (
                                    <button key={g} className={grp === g ? "k-on" : ""} onClick={() => setGrp(g)}>{g}</button>
                                ))}
                            </span>
                            <button className="k-cta k-sec k-fx"><span className="k-lbl">Run</span> <kbd>⌘↵</kbd></button>
                            <span className="k-tipw">
                                <button className="k-cta k-icon k-sec k-fx" aria-label="Info">?</button>
                                <span className="k-tip2">Shows the next hint</span>
                            </span>
                        </div>
                    </div>

                    <div className="k-kc k-rv" style={rise(2).style}>
                        <h3>Buttons: press effects</h3>
                        <p className="k-sub">Click each one. The same four are available to every button in the app.</p>
                        <div className="k-rowf">
                            <Btn>Ripple</Btn>
                            <Btn cls="depth">Depth (3D press)</Btn>
                            <Btn cls="squish">Squish</Btn>
                            <Btn cls="glow">Glow</Btn>
                        </div>
                        <div className="k-rowf">
                            <small>STATES THAT CHANGE</small>
                            <button
                                className={`k-cta k-fx${ld ? ` k-${ld}` : ""}`}
                                onClick={() => {
                                    setLd("busy");
                                    setTimeout(() => {
                                        setLd("done");
                                        setTimeout(() => setLd(""), 1600);
                                    }, 1300);
                                }}
                            >
                                <span className="k-spn" />
                                <span className="k-lbl">Run tests</span>
                                <span className="k-ok">✓ Passed</span>
                            </button>
                            <button
                                className={`k-cta k-sec k-fx${cp ? " k-done" : ""}`}
                                onClick={() => {
                                    setCp(true);
                                    setTimeout(() => setCp(false), 1400);
                                }}
                            >
                                <span className="k-lbl">Copy command</span>
                                <span className="k-ok">✓ Copied</span>
                            </button>
                            <button
                                className={`k-hold${hold ? " k-go" : ""}`}
                                onPointerDown={() => {
                                    setHold(true);
                                    holdTimer.current = setTimeout(() => {
                                        setHold(false);
                                        toast("in", "Progress reset (demo)");
                                    }, 1250);
                                }}
                                onPointerUp={() => (clearTimeout(holdTimer.current), setHold(false))}
                                onPointerLeave={() => (clearTimeout(holdTimer.current), setHold(false))}
                                onPointerCancel={() => (clearTimeout(holdTimer.current), setHold(false))}
                            >
                                <i />
                                <span>Hold to reset progress</span>
                            </button>
                            <span style={{ display: "inline-flex", alignItems: "center", gap: 10 }}>
                                <Sw on={on} onClick={() => setOn(!on)} label="Show hidden tests" />
                                <span style={{ fontSize: 14 }}>Show hidden tests</span>
                            </span>
                            <span className="k-bdg"><span className="k-dot" />passing</span>
                            <span className="k-bdg k-w">in progress</span>
                            <span className="k-bdg k-b">failing</span>
                        </div>
                    </div>

                    <div className="k-kc k-rv" style={rise(3).style}>
                        <h3>Sliders</h3>
                        <p className="k-sub">The filled part follows the thumb; the value updates live; the thumb grows while you hold it.</p>
                        <div className="k-rowf">
                            <Range label="Interview clock" min={10} max={90} step={5} init={45} unit="min" />
                            <Range label="Font size" min={11} max={22} init={15} unit="px" />
                            <span className="k-stp">
                                <button onClick={() => setN(Math.max(1, n - 1))}>−</button>
                                <b>{n}</b>
                                <button onClick={() => setN(Math.min(9, n + 1))}>＋</button>
                            </span>
                        </div>
                    </div>

                    <div className="k-kc k-rv" style={rise(4).style}>
                        <h3>Bars that move</h3>
                        <p className="k-sub">Determinate, striped, indeterminate, shimmer, buffered, steps and a countdown. They fill when this screen opens.</p>
                        <div className="k-bars">
                            <div><div className="k-bl"><span>Progress</span><b>68%</b></div><div className="k-pb"><i style={fill(68)} /></div></div>
                            <div><div className="k-bl"><span>Running tests</span><b>striped, moving</b></div><div className="k-pb k-stripe"><i style={fill(55)} /></div></div>
                            <div><div className="k-bl"><span>Compiling…</span><b>indeterminate</b></div><div className="k-pb k-ind"><i /></div></div>
                            <div><div className="k-bl"><span>Syncing runs</span><b>shimmer</b></div><div className="k-pb k-shim"><i style={fill(40)} /></div></div>
                            <div><div className="k-bl"><span>Loaded / buffered</span><b>34% · 70%</b></div><div className="k-pb k-buf"><i style={fill(34)} /></div></div>
                            <div>
                                <div className="k-bl"><span>Course steps</span><b>{step} of 5</b></div>
                                <div className="k-stepsb">
                                    {Array.from({ length: 5 }, (_, i) => (
                                        <span key={i} style={{ display: "contents" }}>
                                            {i > 0 && <span className={`k-ln${i <= step - 1 ? " k-on" : ""}`}><i /></span>}
                                            <button className={`k-st${i < step - 1 ? " k-on" : i === step - 1 ? " k-cur" : ""}`} onClick={() => setStep(i + 1)}>{i < step - 1 ? "✓" : i + 1}</button>
                                        </span>
                                    ))}
                                </div>
                            </div>
                            <div><div className="k-bl"><span>Interview clock</span><b>counts down</b></div><div className="k-clock"><div className="k-pb"><i style={{ width: `${(100 * secs) / (45 * 60)}%`, transition: "none" }} /></div><b>{mm}</b></div></div>
                        </div>
                    </div>

                    <div className="k-kc k-rv" style={rise(5).style}>
                        <h3>Workspace actions: Run tests and Submit</h3>
                        <p className="k-sub">The two buttons of the Rust workspace's tests pane, with their states. Try Run tests, then Submit; Submit on a failing run shakes and says why, a passing one opens the congratulations popup.</p>
                        <div className="k-rowf">
                            <div className="k-wsp">
                                <div className="k-st3">
                                    <div className="k-row3">
                                        <span className="k-big">{ws.big}</span>
                                        <span className="k-meta2">{ws.meta}</span>
                                    </div>
                                    <div className="k-sg">
                                        {ws.cells.map((c, i) => (
                                            <i key={i} className={c ? `k-${c}` : ""} />
                                        ))}
                                    </div>
                                </div>
                                <div className="k-acts2">
                                    <button className={`k-cta k-sec k-lg k-fx${ws.running ? " k-busy" : ""}`} disabled={ws.running} onClick={() => runWs()}>
                                        <span className="k-spn" /><span className="k-lbl">Run tests</span><span className="k-ok">✓ Done</span><span className="k-kb">⌘↵</span>
                                    </button>
                                    <button className={`k-cta k-lg k-fx${sub ? ` k-${sub}` : ""}`} disabled={ws.running || sub === "busy"} onClick={submit}>
                                        <span className="k-spn" /><span className="k-lbl">Submit</span><span className="k-ok">✓ Submitted</span><span className="k-kb">⇧⌘↵</span>
                                    </button>
                                </div>
                            </div>
                            <div style={{ display: "grid", gap: 10, flex: 1, minWidth: 240 }}>
                                <label style={{ display: "flex", gap: 10, alignItems: "center", fontSize: 14 }}>
                                    <Sw on={ws.fail} onClick={() => setWs((s) => ({ ...s, fail: !s.fail, ok: false }))} label="Make the next run fail" />
                                    Make the next run fail
                                </label>
                                <div style={{ fontSize: 13, color: "var(--dim)" }}>Run tests runs the visible tests. Submit also runs the hidden ones, and is disabled until a run has passed.</div>
                            </div>
                        </div>
                    </div>

                    <div className="k-kc k-rv" style={rise(6).style}>
                        <h3>Popups</h3>
                        <p className="k-sub">Modal (all tests passed, with go-to-next or stay), confirm dialog, popover menu, bottom sheet and a toast stack. Esc, the × button and a click outside all close them.</p>
                        <div className="k-rowf">
                            <Btn onClick={() => setDlg("win")}>Tests passed popup</Btn>
                            <Btn cls="danger" onClick={() => setDlg("lose")}>Confirm dialog</Btn>
                            <span style={{ position: "relative" }} onClick={(e) => e.stopPropagation()}>
                                <Btn cls="sec" onClick={() => setPop(!pop)}>Popover menu ▾</Btn>
                                <div className={`k-pop${pop ? " k-on" : ""}`} style={{ top: "calc(100% + 8px)", left: 0 }}>
                                    <button><span>Run tests</span><kbd>⌘↵</kbd></button>
                                    <button><span>Submit</span><kbd>⇧⌘↵</kbd></button>
                                    <hr />
                                    <button><span>Format code</span><kbd>⌥⇧F</kbd></button>
                                    <button><span>Open scratch file</span></button>
                                    <hr />
                                    <button className="k-dg"><span>Reset to starter</span></button>
                                </div>
                            </span>
                            <Btn cls="sec" onClick={() => setDlg("sheet")}>Bottom sheet</Btn>
                            <Btn cls="tonal" onClick={() => toast("ok", "Tests passed", "All 6 tests pass.")}>Success toast</Btn>
                            <Btn cls="tonal" onClick={() => toast("in", "Optional sections hidden")}>Info toast</Btn>
                            <Btn cls="tonal" onClick={() => toast("er", "4 of 6 passing", "reuses_a_freed_slot failed.")}>Error toast</Btn>
                        </div>
                    </div>
                </div>
            </MockRoot>
            {dlg === "win" && (
                <Celebration
                    title="Stage passed"
                    message="All 6 tests pass. Nicely done: no hints used."
                    stats={[{ value: "6/6", label: "TESTS" }, { value: "1.9s", label: "RUN TIME" }, { value: "0", label: "HINTS" }]}
                    next={{ kicker: "NEXT STAGE", title: "Counters and the log", badge: "MEDIUM" }}
                    goLabel="Go to next stage"
                    onGo={() => setDlg("")}
                    review={{ label: "Review solution", onReview: () => setDlg("") }}
                    onClose={() => setDlg("")}
                />
            )}
            {dlg === "lose" && (
                <Modal onClose={() => setDlg("")} labelledBy="kit-lose" tone="bad">
                    <button className="k-x2" aria-label="Close" onClick={() => setDlg("")}>×</button>
                    <div className="k-ic2">!</div>
                    <h4 id="kit-lose">Reset this stage?</h4>
                    <p>Your code goes back to the starter files. Your passed runs stay recorded.</p>
                    <div className="k-acts">
                        <div className="k-two">
                            <button className="k-cta k-sec k-fx" data-autofocus onClick={() => setDlg("")}><span className="k-lbl">Cancel</span></button>
                            <button className="k-cta k-danger k-fx" onClick={() => setDlg("")}><span className="k-lbl">Reset stage</span></button>
                        </div>
                    </div>
                </Modal>
            )}
            {dlg === "sheet" && (
                <MockRoot>
                    <div className="k-ov k-on" onMouseDown={() => setDlg("")} />
                    <div className="k-sheet k-on">
                        <div className="k-grab" />
                        <h4 style={{ margin: "0 0 4px", fontSize: 18 }}>Share your run</h4>
                        <p style={{ margin: "0 0 14px", color: "var(--mut)", fontSize: 14 }}>A bottom sheet for phones: tap outside to close.</p>
                        <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 10 }}>
                            <button className="k-cta k-sec k-fx" onClick={() => setDlg("")}><span className="k-lbl">Copy link</span></button>
                            <button className="k-cta k-fx" onClick={() => setDlg("")}><span className="k-lbl">Done</span></button>
                        </div>
                    </div>
                </MockRoot>
            )}
            <SharedControls />
        </>
    );
}

/** The shared controls of components/ui.tsx and components/icons.tsx, with every state, as the reference for the rest of the app. */
function SharedControls() {
    const [sw, setSw] = useState(true);
    const [seg, setSeg] = useState<"a" | "b" | "c">("a");
    return (
        <section style={{ maxWidth: 900, margin: "40px auto", padding: "0 24px", display: "flex", flexDirection: "column", gap: 18 }} aria-label="Shared controls">
            <h2 style={{ margin: 0 }}>Shared controls</h2>
            <div style={{ display: "flex", gap: 10, flexWrap: "wrap", alignItems: "center" }}>
                <UiBtn variant="primary">Primary</UiBtn>
                <UiBtn>Secondary</UiBtn>
                <UiBtn variant="quiet">Quiet</UiBtn>
                <UiBtn variant="danger">Danger</UiBtn>
                <UiBtn disabled>Disabled</UiBtn>
            </div>
            <div style={{ display: "flex", gap: 18, flexWrap: "wrap", alignItems: "center" }}>
                <Switch on={sw} onChange={setSw} label="Example switch" />
                <Switch on={false} onChange={() => undefined} label="Off" />
                <Switch on disabled onChange={() => undefined} label="Disabled" />
                <Segmented label="Example" value={seg} options={[["a", "One"], ["b", "Two"], ["c", "Three"]] as const} onChange={setSeg} />
                <ConfirmButton label="Delete something" confirm="Yes, delete it" note="It cannot be undone here." onConfirm={() => undefined} />
            </div>
            <div style={{ display: "flex", gap: 22, fontSize: 22 }} aria-label="Log icons">
                <span style={{ color: "var(--grn)" }}><GradeGlyph grade="good" /></span>
                <span style={{ color: "var(--warn)" }}><GradeGlyph grade="hard" /></span>
                <span style={{ color: "var(--warn)" }}><GradeGlyph grade="again" /></span>
                <span style={{ color: "var(--vio)" }}><GradeGlyph grade="easy" /></span>
            </div>
        </section>
    );
}

type ReactLabel = string;
