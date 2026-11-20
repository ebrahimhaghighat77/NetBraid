import { useId, type CSSProperties } from "react";
import { braided, crossings, loose, near, path, type Geometry } from "../lib/braid";

export interface Strand {
  color: string;
  width: number;
  /** Seconds per dash cycle of the flowing highlight; none when still. */
  flow?: number;
}

interface Props {
  g: Geometry;
  strands: Strand[];
  out: { color: string; width: number; flow?: number } | null;
  gap: number;
  gapColor: string;
  apart?: boolean;
}

const morph = (d: string) => ({ d: `path("${d}")` }) as CSSProperties;

/** Strands that twist into one line, with each crossing drawn over or under. */
export function Strands({ g, strands, out, gap, gapColor, apart = false }: Props) {
  const id = useId().replace(/:/g, "");
  const n = strands.length;
  const geometry = { ...g, strands: n };
  const lines = strands.map((_, i) => braided(geometry, i));
  const shown = strands.map((_, i) => (apart ? loose(geometry, i) : lines[i]));
  const cross = n > 1 ? crossings(geometry) : [];
  const widest = Math.max(...strands.map((s) => s.width), 1);

  return (
    <g className="strands" data-apart={apart}>
      <defs>
        {lines.map((pts, i) => (
          <mask key={i} id={`${id}u${i}`} maskUnits="userSpaceOnUse">
            <path d={path(pts)} stroke="#fff" strokeWidth={strands[i].width + 0.6} fill="none" strokeLinecap="round" />
          </mask>
        ))}
      </defs>
      {shown.map((pts, i) => (
        <path
          key={i}
          className="strand"
          style={morph(path(pts))}
          stroke={strands[i].color}
          strokeWidth={strands[i].width}
          fill="none"
          strokeLinecap="round"
          strokeLinejoin="round"
        />
      ))}
      <g className="weave">
        {cross.map((c, k) => {
          const room = Math.min(...cross.filter((o) => o !== c).map((o) => Math.abs(o.x - c.x)), Infinity);
          const reach = Math.min(widest * 1.3, room * 0.45);
          const seg = path(near(lines[c.over], c.x, reach));
          const under = lines
            .map((pts, i) => ({ i, dy: Math.abs((pts.find(([x]) => x >= c.x)?.[1] ?? 0) - (lines[c.over].find(([x]) => x >= c.x)?.[1] ?? 0)) }))
            .filter((u) => u.i !== c.over)
            .sort((a, b) => a.dy - b.dy)[0].i;
          return (
            <g key={k}>
              <path d={seg} stroke={gapColor} strokeWidth={strands[c.over].width + 2 * gap} fill="none" strokeLinecap="round" mask={`url(#${id}u${under})`} />
              <path d={seg} stroke={strands[c.over].color} strokeWidth={strands[c.over].width} fill="none" strokeLinecap="round" />
            </g>
          );
        })}
      </g>
      {strands.map((s, i) =>
        s.flow ? (
          <path key={i} className="flow" style={{ ...morph(path(shown[i])), animationDuration: `${s.flow}s` }} fill="none" />
        ) : null,
      )}
      {out && (
        <g className="out">
          <path d={`M${g.merge - 2} ${g.cy}H${g.x1}`} stroke={out.color} strokeWidth={out.width} strokeLinecap="round" />
          {out.flow && (
            <path className="flow" d={`M${g.merge - 2} ${g.cy}H${g.x1}`} style={{ animationDuration: `${out.flow}s` }} fill="none" />
          )}
        </g>
      )}
    </g>
  );
}
