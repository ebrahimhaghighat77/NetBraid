import { useEffect, useRef, useState, type CSSProperties } from "react";
import { ChevronDown, EthernetPort, Network, Smartphone, Wifi } from "lucide-react";
import { api } from "../core/api";
import type { Kind, LinkView, Period, Snapshot } from "../core/types";
import { capPercent, capText, linkState } from "../lib/netstate";
import { Switch } from "./Switch";

const ICONS: Record<Kind, typeof Wifi> = {
  wifi: Wifi,
  ethernet: EthernetPort,
  phone: Smartphone,
  cellular: Smartphone,
  other: Network,
};
const WEIGHT_DELAY_MS = 250;

interface Props {
  link: LinkView;
  color: string;
  live: boolean;
  open: boolean;
  onToggle: () => void;
  run: (change: Promise<Snapshot>) => void;
}

export function NetworkCard({ link, color, live, open, onToggle, run }: Props) {
  const state = linkState(link, live);
  const idle = !link.present || !link.enabled || link.capped;
  const Icon = ICONS[link.kind];

  return (
    <div className={idle ? "net idle" : "net"} style={{ "--c": color } as CSSProperties}>
      <div className="net-row">
        <button type="button" className="net-open" aria-expanded={open} aria-label={`${link.name} options`} onClick={onToggle}>
          <Icon size={16} />
          <span className="net-info">
            <span className="net-title">
              <span className="net-name" title={link.description}>
                {link.name}
              </span>
              {link.backup && <span className="chip">Backup</span>}
              {link.virtualAdapter && <span className="chip">Virtual</span>}
              {link.present && link.reach === "local" && <span className="chip warn">In-country only</span>}
            </span>
            <span className="net-meta">{link.present ? link.address : link.description}</span>
          </span>
          <ChevronDown size={14} className="chev" />
        </button>
        <div className="net-rate">
          <span className="num">{state.value}</span>
          <span className="muted">{state.detail}</span>
        </div>
        <Switch
          on={link.enabled && link.present}
          label={`Use ${link.name}`}
          disabled={!link.present}
          onChange={(enabled) => run(api.linkUpdate(link.id, { enabled }))}
        />
      </div>
      {link.capMb != null && (
        <div className="cap">
          <div className="cap-track">
            <div className={link.capped ? "cap-fill full" : "cap-fill"} style={{ width: `${capPercent(link)}%` }} />
          </div>
          <span>{capText(link)}</span>
        </div>
      )}
      {open && <Options link={link} run={run} />}
    </div>
  );
}

function Options({ link, run }: Pick<Props, "link" | "run">) {
  return (
    <div className="net-panel">
      <Weight link={link} run={run} />
      <div className="field">
        <span className="field-text">
          <span>Backup only</span>
          <span className="field-desc">Used when no other network works.</span>
        </span>
        <Switch
          on={link.backup}
          label={`Keep ${link.name} as a backup`}
          onChange={(backup) => run(api.linkUpdate(link.id, { backup }))}
        />
      </div>
      <Cap link={link} run={run} />
      <dl className="facts">
        <dt>Address</dt>
        <dd>{link.address ?? "—"}</dd>
        <dt>Gateway</dt>
        <dd>{link.gateway ?? "—"}</dd>
        <dt>Latency</dt>
        <dd>{link.latencyMs != null ? `${link.latencyMs} ms` : "—"}</dd>
        <dt>Adapter</dt>
        <dd title={link.description}>{link.description || "—"}</dd>
      </dl>
    </div>
  );
}

function Weight({ link, run }: Pick<Props, "link" | "run">) {
  const [draft, setDraft] = useState<number | null>(null);
  const timer = useRef<number | undefined>(undefined);
  useEffect(() => () => window.clearTimeout(timer.current), []);

  const value = draft ?? link.weight;
  const slide = (next: number) => {
    setDraft(next);
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => {
      run(api.linkUpdate(link.id, { weight: next }).finally(() => setDraft(null)));
    }, WEIGHT_DELAY_MS);
  };

  return (
    <div className="field stack">
      <div className="field-line">
        <span>Share of new connections</span>
        <span className="num">{value} / 10</span>
      </div>
      <input
        type="range"
        className="slider"
        min={1}
        max={10}
        step={1}
        value={value}
        aria-label={`Share for ${link.name}`}
        style={{ "--fill": `${((value - 1) / 9) * 100}%` } as CSSProperties}
        onChange={(e) => slide(Number(e.target.value))}
      />
    </div>
  );
}

function Cap({ link, run }: Pick<Props, "link" | "run">) {
  const [gb, setGb] = useState(link.capMb != null ? String(link.capMb / 1000) : "");
  const [period, setPeriod] = useState<Period>(link.capPeriod);

  const typed = gb.trim() === "" ? null : Number(gb);
  const valid = typed === null || (Number.isFinite(typed) && typed > 0);
  const capMb = typed === null || !valid ? null : Math.round(typed * 1000);
  const changed = capMb !== link.capMb || (capMb !== null && period !== link.capPeriod);
  const save = () => {
    if (valid && changed) run(api.linkCap(link.id, capMb, period));
  };

  return (
    <div className="field stack">
      <span className="field-text">
        <span>Data cap</span>
        <span className="field-desc">Stops using this network once it has carried this much.</span>
      </span>
      <div className="field-line">
        <span className="cap-edit">
          <input
            className="input short"
            inputMode="decimal"
            placeholder="None"
            value={gb}
            aria-label={`Data cap for ${link.name} in gigabytes`}
            aria-invalid={!valid}
            onChange={(e) => setGb(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && save()}
          />
          <span className="muted">GB per</span>
          <select className="select short" value={period} aria-label="Cap period" onChange={(e) => setPeriod(e.target.value as Period)}>
            <option value="day">day</option>
            <option value="month">month</option>
          </select>
        </span>
        <span className="cap-edit">
          {link.capMb != null && (
            <button type="button" className="btn" title="Start counting again, for a renewed data plan" onClick={() => run(api.usageReset(link.id))}>
              Reset
            </button>
          )}
          <button type="button" className="btn" disabled={!valid || !changed} onClick={save}>
            Save
          </button>
        </span>
      </div>
    </div>
  );
}
