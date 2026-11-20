import type { LinkView, Snapshot } from "../core/types";
import { bytes, count } from "./format";

export type Switching = "on" | "off" | null;
export type Tone = "ok" | "warn" | "idle";

export function inUse(links: LinkView[]): LinkView[] {
  return links.filter((l) => l.present && l.enabled && !l.capped);
}

export function headline(s: Snapshot, switching: Switching): { text: string; tone: Tone } {
  const pending = switching ?? (s.busy ? (s.on ? "off" : "on") : null);
  if (pending) return { text: pending === "on" ? "Turning on…" : "Turning off…", tone: "idle" };
  if (!s.on) {
    const own = s.links.find((l) => l.isDefault);
    return { text: own ? `Off, Windows uses ${own.name} alone` : "Off", tone: s.error ? "warn" : "idle" };
  }
  const working = inUse(s.links).filter((l) => l.reach !== "down").length;
  if (working === 0) return { text: "On, but no network has internet", tone: "warn" };
  return {
    text: working === 1 ? "On, using 1 network" : `Combining ${working} networks`,
    tone: s.error ? "warn" : "ok",
  };
}

export function footerText(s: Snapshot): string {
  if (s.on) return `${count(s.connections, "connection")}, ${bytes(s.todayBytes)} today`;
  return `${count(s.links.filter((l) => l.present).length, "network")} ready`;
}
