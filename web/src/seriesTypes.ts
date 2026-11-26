// What /api/series answers (crates/api/src/series.rs).

export type SeriesItemState = "solved" | "assisted" | "next" | "started" | "open" | "to_write";

/** A preparation part or a capstone step. `id` is absent while it is to be written. */
export interface SeriesItem {
  kind: "part" | "step";
  number: number;
  id: string | null;
  title: string;
  idea: string;
  minutes: number | null;
  state: SeriesItemState;
}

export type SeriesRow =
  | ({ row: "item" } & SeriesItem)
  | { row: "repeat"; number: number; id: string; state: "waiting" | "scheduled" | "due" | "done"; due: string | null; minutes: number | null };

export interface SeriesSummary {
  id: string;
  title: string;
  summary: string;
  /** Whether each part is solved, in order. */
  segments: boolean[];
  solved: number;
  to_write: number;
  steps: number;
  steps_solved: number;
  next: SeriesItem | null;
}

export interface SeriesView extends SeriesSummary {
  next_why: string;
  groups: { name: string; rows: SeriesRow[] }[];
  capstone: { title: string; summary: string; note: string; cookbook: { id: string; title: string } | null; steps: SeriesItem[] } | null;
  note: string;
  needs: { need: string; parts: string; concepts: { concept: string; parts: string; steps: string; note: string }[] }[];
  ideas: { name: string; learn: string; again: string; apply: string }[];
  optional: { id: string; title: string; why: string }[];
}

export interface SeriesLink {
  id: string | null;
  label: string;
  number: number | null;
  title: string;
  why: string;
  reveals_method: boolean;
}

export interface SeriesDone {
  title: string;
  practised: { label: string; text: string }[];
  returns: { label: string; text: string }[];
  repeat: { text: string; due: string | null }[];
  next: SeriesItem | null;
  skipped: string[];
}

/** Where a problem sits in a series. */
export interface SeriesPlace {
  series: { id: string; title: string };
  /** The other series that have this problem; usually none. */
  others: { id: string; title: string; kind: "part" | "step"; number: number }[];
  kind: "part" | "step";
  number: number;
  total: number;
  /** The problems either side in the series' order (parts, then capstone steps); null at the ends. */
  prev: string | null;
  next: string | null;
  minutes: number | null;
  segments: boolean[];
  know_first: string | null;
  required: SeriesLink[];
  helpful: SeriesLink[];
  cookbook: { page: string; title: string; level: "exact" | "similar" | "related" }[];
  /** Only once the problem is solved. */
  done: SeriesDone | null;
}
