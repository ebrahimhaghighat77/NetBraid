/** Three networks braided into one line. */
export function Mark({ lit = true, size = 26 }: { lit?: boolean; size?: number }) {
  const strand = (color: string) => (lit ? color : "var(--text-3)");
  return (
    <svg className="mark" width={size} height={size} viewBox="0 0 26 26" fill="none" aria-hidden>
      <g strokeWidth="2.4" strokeLinecap="round">
        <path d="M3 5.5c7 0 5 7.5 12 7.5" stroke={strand("var(--net-wifi)")} />
        <path d="M3 13h12" stroke={strand("var(--net-eth)")} />
        <path d="M3 20.5c7 0 5-7.5 12-7.5" stroke={strand("var(--net-phone)")} />
        <path d="M15 13h8" stroke={lit ? "var(--accent)" : "var(--text-2)"} strokeWidth="3" />
      </g>
    </svg>
  );
}
