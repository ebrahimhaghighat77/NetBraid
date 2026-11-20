export type Point = [number, number];

export interface Geometry {
  strands: number;
  x0: number;
  merge: number;
  x1: number;
  cy: number;
  amp: number;
  period: number;
  step: number;
  lead: number;
  phase: number;
}

/** The starting phase that spreads `n` strands most evenly where they enter. */
export function evenPhase(n: number): number {
  let best = 0;
  let widest = -1;
  for (let k = 0; k < 720; k++) {
    const phase = (k / 720) * 2 * Math.PI;
    const ys = Array.from({ length: n }, (_, i) => Math.sin(phase + (2 * Math.PI * i) / n)).sort((a, b) => a - b);
    const gap = Math.min(...ys.slice(1).map((y, i) => y - ys[i]), Infinity);
    if (gap > widest + 1e-9) {
      widest = gap;
      best = phase;
    }
  }
  return best;
}

export interface Crossing {
  x: number;
  over: number;
}

function xs(g: Geometry): number[] {
  const out: number[] = [];
  for (let x = g.x0; x < g.x1; x += g.step) out.push(x);
  out.push(g.x1);
  return out;
}

function swing(g: Geometry, x: number): number {
  const fade = g.x0 + (g.merge - g.x0) * 0.4;
  if (x <= fade) return 1;
  if (x >= g.merge) return 0;
  return 0.5 * (1 + Math.cos((Math.PI * (x - fade)) / (g.merge - fade)));
}

/** How far the twist has turned by `x`: it starts still and speeds up over `lead`. */
function travel(g: Geometry, x: number): number {
  const run = x - g.x0;
  if (g.lead <= 0) return run;
  if (run >= g.lead) return run - g.lead / 2;
  const u = Math.max(0, run / g.lead);
  return g.lead * (u ** 3 - u ** 4 / 2);
}

/** Strands enter side by side, twist around each other, and meet in one line. */
export function braided(g: Geometry, i: number): Point[] {
  const turn = g.phase + (2 * Math.PI * i) / Math.max(g.strands, 1);
  return xs(g).map((x) => [
    x,
    g.cy + g.amp * swing(g, x) * Math.sin((2 * Math.PI * travel(g, x)) / g.period + turn),
  ]);
}

export function loose(g: Geometry, i: number): Point[] {
  const turn = g.phase + (2 * Math.PI * i) / Math.max(g.strands, 1);
  return xs(g).map((x) => [x, g.cy + g.amp * Math.sin(turn)]);
}

export function crossings(g: Geometry): Crossing[] {
  const lines = Array.from({ length: g.strands }, (_, i) => braided(g, i));
  const found: { x: number; down: number; up: number }[] = [];
  const stop = g.merge - g.period * 0.2;
  for (let a = 0; a < g.strands; a++) {
    for (let b = a + 1; b < g.strands; b++) {
      for (let k = 1; k < lines[a].length && lines[a][k][0] < stop; k++) {
        const before = lines[a][k - 1][1] - lines[b][k - 1][1];
        const after = lines[a][k][1] - lines[b][k][1];
        if (before === 0 || Math.sign(before) === Math.sign(after)) continue;
        const aDown = lines[a][k][1] > lines[a][k - 1][1];
        found.push({ x: lines[a][k][0], down: aDown ? a : b, up: aDown ? b : a });
      }
    }
  }
  return found
    .sort((p, q) => p.x - q.x)
    .map((c, n) => ({ x: c.x, over: n % 2 === 0 ? c.down : c.up }));
}

export function near(points: Point[], x: number, reach: number): Point[] {
  return points.filter(([px]) => Math.abs(px - x) <= reach);
}

export function path(points: Point[]): string {
  return points.map(([x, y], i) => `${i ? "L" : "M"}${x.toFixed(2)} ${y.toFixed(2)}`).join("");
}
