// The key idea of a problem and its technique name give the solution away, so lists, cards and a problem's own header hide them until you ask.
// "Show the key idea" in Settings turns that off for good; a click shows one. The choice is a preference, so it follows you to another laptop.
import { useEffect, useState } from "react";
import { getPref, setPref } from "./prefs";

const KEY = "spoilers.show";

export function useShowIdeas(): [boolean, (v: boolean) => void] {
    const [show, setShow] = useState(() => getPref(KEY, false));
    useEffect(() => {
        const f = (e: Event) => (e as CustomEvent<string>).detail === KEY && setShow(getPref(KEY, false));
        window.addEventListener("anneal:pref", f);
        return () => window.removeEventListener("anneal:pref", f);
    }, []);
    return [show, (v) => setPref(KEY, v)];
}
