import type { LinkView } from "../core/types";
import { bytes, rate } from "./format";

function unavailable(link: LinkView): string {
  switch (link.reason) {
    case "no IP address":
      return "No IP address";
    case "no route to the internet":
      return "No route to the internet";
    default:
      return link.kind === "ethernet" ? "Cable unplugged" : "Not connected";
  }
}

export function linkState(link: LinkView, live: boolean): { value: string; detail: string } {
  if (!link.present) return { value: "—", detail: unavailable(link) };
  if (link.capped) return { value: "—", detail: "Data cap reached" };
  if (!link.enabled) return { value: "—", detail: "Switched off" };
  if (link.reach === "down") return { value: "—", detail: "No internet" };
  if (!live) {
    return {
      value: link.latencyMs != null ? `${link.latencyMs} ms` : "—",
      detail: link.isDefault ? "Windows default" : "Ready",
    };
  }
  if (link.backup && link.connections === 0) return { value: "—", detail: "Standing by" };
  return { value: rate(link.rxRate), detail: `${link.share}% of traffic` };
}

export const capBytes = (capMb: number) => capMb * 1_000_000;

export function capText(link: LinkView): string {
  if (link.capMb == null) return "";
  const when = link.capPeriod === "day" ? "today" : "this month";
  return `${bytes(link.usedBytes)} of ${bytes(capBytes(link.capMb))} ${when}`;
}

export function capPercent(link: LinkView): number {
  if (link.capMb == null || link.capMb <= 0) return 0;
  return Math.min(100, (link.usedBytes / capBytes(link.capMb)) * 100);
}
