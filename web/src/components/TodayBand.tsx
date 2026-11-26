import { Link } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { api } from "../api";
import { WEEKDAYS, newOn, parseDay } from "../dsa";

/** What to do today, in order: the reviews, then the new problem, then logging it. Shown above the Plan and the Calendar. It reads
 *  the same overview and activity as the header's "today" chip. */
export function TodayBand() {
    const o = useQuery({ queryKey: ["dsa"], queryFn: api.dsa }).data;
    const activity = useQuery({ queryKey: ["activity", "header"], queryFn: () => api.activity(["D"]) }).data;
    if (!o || !activity) return null;
    const plan = o.plan;
    const done = activity.week[activity.week.length - 1]?.solved ?? 0;
    const target = plan.solve_day ? newOn(o.settings, WEEKDAYS[(parseDay(o.today).getDay() + 6) % 7]!) : 0;
    const next = o.problems.find((x) => x.id === plan.next_up[0]);
    const step = plan.due > 0 ? 1 : target > 0 && done < target && next ? 2 : 0;
    const reviews = `${plan.due} review${plan.due === 1 ? "" : "s"}`;
    return (
        <section className="rsd-today" aria-label="Today">
            <div className="rsd-now">
                <p className="rsd-kick">Today · {plan.solve_day ? "solve day" : plan.capacity > 0 ? "practice day" : "rest day"}</p>
                {step === 1 ? (
                    <>
                        <h2>Next: clear <span className="rsd-grad">{reviews}</span></h2>
                        <p>Reviews come first. Recall the approach, then grade how it went.</p>
                        <Link className="rsd-go" to="/dsa/review">Start the reviews</Link>
                    </>
                ) : step === 2 && next ? (
                    <>
                        <h2>Next: solve <span className="rsd-grad">{next.title}</span></h2>
                        <p>Your next new problem. Solve it on LeetCode, then log how it went.</p>
                        <Link className="rsd-go" to="/d/$slug" params={{ slug: next.slug }}>Open the problem</Link>
                    </>
                ) : (
                    <>
                        <h2>{target > 0 && done >= target ? "Today's new problems are done." : "Nothing is due now."}</h2>
                        <p>{target > 0 && done >= target ? `${done} of ${target} solved today, and no review is waiting.` : "No new problem is planned today, and no review is waiting."}</p>
                    </>
                )}
            </div>
            <ol className="rsd-order">
                <li className={step === 1 ? "now" : "done"}>
                    <b>1</b>
                    <div><strong>Reviews</strong><span>{plan.due} due</span></div>
                </li>
                <li className={step === 2 ? "now" : target > 0 && done >= target ? "done" : ""}>
                    <b>2</b>
                    <div><strong>New problem</strong><span>{target === 0 ? "none planned today" : done >= target ? `${done} done today` : `${target - done} planned today`}</span></div>
                </li>
                <li>
                    <b>3</b>
                    <div><strong>Log it</strong><span>keys 1 to 4 on the problem page</span></div>
                </li>
            </ol>
        </section>
    );
}
