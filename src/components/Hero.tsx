import { useMemo } from "react";
import type { LinkView } from "../core/types";
import { evenPhase, type Geometry } from "../lib/braid";
import { Strands, type Strand } from "./Strands";

const W = 328;
const H = 112;
const SPEEDS: [number, number][] = [
  [4_000_000, 0.55],
  [1_000_000, 0.85],
  [250_000, 1.25],
  [0, 1.9],
];

const flowFor = (rate: number) => (rate > 1024 ? SPEEDS.find(([min]) => rate >= min)![1] : undefined);

/** Every network as a strand as thick as its share, braided into one line while on. */
export function Hero({ links, colors, on }: { links: LinkView[]; colors: Map<string, string>; on: boolean }) {
  const n = links.length;
  const g: Geometry = useMemo(
    () => ({
      strands: n,
      x0: 10,
      merge: 238,
      x1: 318,
      cy: H / 2,
      amp: n > 1 ? 38 : 0,
      period: 150,
      step: 3,
      lead: 72,
      phase: evenPhase(Math.max(n, 1)),
    }),
    [n],
  );
  const strands: Strand[] = links.map((l) => ({
    color: on ? (colors.get(l.id) ?? "var(--text-3)") : "var(--text-3)",
    width: on ? 3 + (13 * l.share) / 100 : 2.5,
    flow: on ? flowFor(l.rxRate) : undefined,
  }));
  const total = links.reduce((sum, l) => sum + l.rxRate, 0);
  const thick = strands.reduce((sum, s) => sum + s.width, 0);
  const out = on ? { color: "var(--accent)", width: Math.min(20, Math.max(6, thick * 0.6)), flow: flowFor(total) } : null;
  const glow = on ? Math.min(0.38, 0.14 + total / 16_000_000) : 0;

  return (
    <svg className="braid" viewBox={`0 0 ${W} ${H}`} role="img" aria-label={`${n} networks ${on ? "braided into one" : "side by side"}`}>
      <defs>
        <radialGradient id="knot-glow">
          <stop offset="0" stopColor="var(--accent)" stopOpacity="1" />
          <stop offset="1" stopColor="var(--accent)" stopOpacity="0" />
        </radialGradient>
      </defs>
      <circle className="knot" cx={g.merge + 6} cy={g.cy} r={40} fill="url(#knot-glow)" style={{ opacity: glow }} />
      <Strands g={g} strands={strands} out={out} gap={2.4} gapColor="rgb(var(--ink))" apart={!on} />
    </svg>
  );
}
