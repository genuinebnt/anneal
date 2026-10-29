import { useEffect } from "react";
import { getPref, setPref } from "../prefs";
import { LATEST } from "../whatsnew";
import { toast } from "./toasts";

/** Once after an update, a toast says there is something new and links to the list in Settings. Mounted once, at the root. */
export function WhatsNewToast() {
    useEffect(() => {
        if (navigator.webdriver || location.pathname === "/login") return;
        if (getPref("whatsnew.toasted", "") === LATEST) return;
        setPref("whatsnew.toasted", LATEST);
        toast("in", "anneal was updated", "See what is new.", { label: "What's new", run: () => location.assign("/settings#new") });
    }, []);
    return null;
}
