export const SAMPLES = 60;

/** The last `SAMPLES` values, padded with zeros on the left. */
export function window60(history: number[]): number[] {
  const tail = history.slice(-SAMPLES);
  return [...new Array<number>(SAMPLES - tail.length).fill(0), ...tail];
}

export function totals(series: number[][]): number[] {
  return Array.from({ length: SAMPLES }, (_, t) => series.reduce((sum, s) => sum + s[t], 0));
}

type Point = [number, number];

function curve(points: Point[]): string {
  let d = "";
  for (let i = 1; i < points.length; i++) {
    const [x0, y0] = points[i - 1];
    const [x1, y1] = points[i];
    const xm = (x0 + x1) / 2;
    d += `C${xm.toFixed(1)},${y0.toFixed(1)} ${xm.toFixed(1)},${y1.toFixed(1)} ${x1.toFixed(1)},${y1.toFixed(1)}`;
  }
  return d;
}

const start = ([x, y]: Point) => `${x.toFixed(1)},${y.toFixed(1)}`;

export function linePath(points: Point[]): string {
  return `M${start(points[0])}${curve(points)}`;
}

export function bandPath(upper: Point[], lower: Point[]): string {
  const back = [...lower].reverse();
  return `M${start(upper[0])}${curve(upper)}L${start(back[0])}${curve(back)}Z`;
}
