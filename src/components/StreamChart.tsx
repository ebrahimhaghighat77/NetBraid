import type { CSSProperties } from "react";
import type { LinkView } from "../core/types";
import { SAMPLES, bandPath, linePath, totals, window60 } from "../lib/stream";

const W = 328;
const H = 78;
const TOP = 8;
// One sample sits off the right edge and slides in over the next second.
const STEP = W / (SAMPLES - 2);
const FLOOR = 64 * 1024;

export function StreamChart({ links, colors }: { links: LinkView[]; colors: Map<string, string> }) {
  const series = links.map((l) => window60(l.history));
  const sum = totals(series);
  const peak = Math.max(FLOOR, ...sum);
  const x = (t: number) => (t - 1) * STEP;
  const y = (v: number) => H - (v / peak) * (H - TOP);

  let below = new Array<number>(SAMPLES).fill(0);
  const bands = links.map((link, i) => {
    const above = below.map((v, t) => v + series[i][t]);
    const d = bandPath(
      above.map((v, t) => [x(t), y(v)]),
      below.map((v, t) => [x(t), y(v)]),
    );
    below = above;
    return { id: link.id, d };
  });
  const top = linePath(sum.map((v, t) => [x(t), y(v)]));
  const tick = sum.slice(-4).join(":");

  return (
    <svg className="stream" viewBox={`0 0 ${W} ${H}`} role="img" aria-label="Download speed of each network over the last minute">
      <g key={tick} className="stream-slide" style={{ "--step": `${STEP}px` } as CSSProperties}>
        {bands.map((band) => (
          <path key={band.id} d={band.d} fill={colors.get(band.id)} fillOpacity={0.85} />
        ))}
        <path d={top} fill="none" stroke="var(--text)" strokeWidth={1.25} strokeOpacity={0.9} />
      </g>
    </svg>
  );
}
