import { invoke } from "@tauri-apps/api/core";
import type {
  FlowView,
  LinkChange,
  LinkTarget,
  OptionChange,
  Period,
  Rule,
  Snapshot,
} from "./types";

export const api = {
  snapshot: () => invoke<Snapshot>("snapshot"),
  power: (on: boolean) => invoke<Snapshot>("power", { on }),
  linkUpdate: (id: string, change: LinkChange) => invoke<Snapshot>("link_update", { id, change }),
  linkCap: (id: string, capMb: number | null, period: Period) =>
    invoke<Snapshot>("link_cap", { id, capMb, period }),
  usageReset: (id: string) => invoke<Snapshot>("usage_reset", { id }),
  rulesSave: (rules: Rule[]) => invoke<Snapshot>("rules_save", { rules }),
  optionsSave: (change: OptionChange) => invoke<Snapshot>("options_save", { change }),
  flows: () => invoke<FlowView[]>("flows_list"),
  log: () => invoke<string[]>("log_lines"),
  updateCheck: () => invoke<Snapshot>("update_check"),
  updateInstall: () => invoke<void>("update_install"),
  openLink: (target: LinkTarget) => invoke<void>("open_link", { target }),
  fit: (height: number) => invoke<void>("popup_fit", { height }),
  hide: () => invoke<void>("popup_hide"),
  quit: () => invoke<void>("quit"),
};

export function message(err: unknown): string {
  const text = typeof err === "string" ? err : err instanceof Error ? err.message : String(err);
  return text.charAt(0).toUpperCase() + text.slice(1);
}
