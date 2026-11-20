import type { Kind, LinkView } from "../core/types";

const OWN: Record<Kind, string> = {
  wifi: "var(--net-wifi)",
  ethernet: "var(--net-eth)",
  phone: "var(--net-phone)",
  cellular: "var(--net-phone)",
  other: "var(--net-spare-4)",
};
const SPARE = ["var(--net-spare-1)", "var(--net-spare-2)", "var(--net-spare-3)", "var(--net-spare-4)"];

/**
 * One colour per network: the first of each kind gets the kind's colour, later ones a spare.
 * Ordered by adapter id, so colours stay put as networks come and go.
 */
export function linkColors(links: LinkView[]): Map<string, string> {
  const taken = new Set<string>();
  const colors = new Map<string, string>();
  let spare = 0;
  for (const link of [...links].sort((a, b) => a.id.localeCompare(b.id))) {
    const own = OWN[link.kind];
    if (taken.has(own)) {
      colors.set(link.id, SPARE[spare++ % SPARE.length]);
    } else {
      taken.add(own);
      colors.set(link.id, own);
    }
  }
  return colors;
}
