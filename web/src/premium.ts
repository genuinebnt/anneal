// Premium problems are shown and counted together, or hidden and not counted together: one switch, kept in the goal's `free_only`
// (hidden = true). Hidden problems stay logged; completing one still counts toward the streak and activity.
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "./api";

/** The keys of every query that shows a count or a list of problems, so a change to the switch is seen everywhere at once. */
export const COUNT_KEYS = ["settings", "dsa", "dsa-row", "tracks", "progress", "activity", "stats", "reviews", "dsa-review", "practice", "plan"];

export function usePremiumSwitch() {
    const qc = useQueryClient();
    const settings = useQuery({ queryKey: ["settings"], queryFn: api.settings });
    const srs = settings.data?.srs;
    const save = useMutation({
        mutationFn: (hidden: boolean) => api.saveSrs({ ...srs!, goal: { ...srs!.goal, free_only: hidden } }),
        onSuccess: () => COUNT_KEYS.forEach((k) => void qc.invalidateQueries({ queryKey: [k] })),
    });
    return { ready: !!srs, hidden: srs?.goal.free_only ?? true, saving: save.isPending, error: save.isError ? (save.error as Error).message : null, set: (hidden: boolean) => srs && save.mutate(hidden) };
}
