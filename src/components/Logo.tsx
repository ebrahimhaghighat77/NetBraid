import { evenPhase, type Geometry } from "../lib/braid";
import { Strands } from "./Strands";

const G: Geometry = { strands: 3, x0: 16, merge: 72, x1: 86, cy: 50, amp: 22, period: 78, step: 1, lead: 30, phase: evenPhase(3) };
const LIT = ["var(--net-eth)", "var(--net-phone)", "var(--net-wifi)"];

export function Logo({ size = 28, lit = true }: { size?: number; lit?: boolean }) {
  const color = (c: string) => (lit ? c : "var(--text-3)");
  return (
    <svg className="logo" width={size} height={size} viewBox="8 18 84 64" aria-hidden>
      <Strands
        g={G}
        strands={LIT.map((c) => ({ color: color(c), width: 10 }))}
        out={{ color: lit ? "var(--accent)" : "var(--text-2)", width: 14 }}
        gap={3.5}
        gapColor="rgb(var(--ink))"
      />
    </svg>
  );
}
