import { useState } from "react";
import { ArrowUp, CircleArrowUp, ShieldCheck } from "lucide-react";
import type { Page } from "../App";
import { api, message } from "../core/api";
import type { Snapshot } from "../core/types";
import { Footer } from "../components/Footer";
import { Header } from "../components/Header";
import { NetworkCard } from "../components/NetworkCard";
import { StreamChart } from "../components/StreamChart";
import { linkColors } from "../lib/colors";
import { rate, splitRate } from "../lib/format";
import { totals, window60 } from "../lib/stream";
import { footerText, headline, inUse, type Switching } from "../lib/summary";
import { useEased } from "../lib/useEased";

interface Props {
  snapshot: Snapshot;
  onSnapshot: (next: Snapshot) => void;
  onOpen: (page: Page) => void;
}

export function Home({ snapshot, onSnapshot, onOpen }: Props) {
  const [switching, setSwitching] = useState<Switching>(null);
  const [error, setError] = useState<string | null>(null);
  const [openId, setOpenId] = useState<string | null>(null);

  const run = (change: Promise<Snapshot>) => {
    setError(null);
    change.then(onSnapshot).catch((e) => setError(message(e)));
  };
  const power = (on: boolean) => {
    setSwitching(on ? "on" : "off");
    run(api.power(on).finally(() => setSwitching(null)));
  };

  const status = headline(snapshot, switching);
  const busy = switching !== null || snapshot.busy;
  const notice = error ?? (snapshot.error && message(snapshot.error));
  const colors = linkColors(snapshot.links);
  const shown = snapshot.settings.hideUnavailable ? snapshot.links.filter((l) => l.present) : snapshot.links;

  return (
    <>
      <Header on={snapshot.on} status={status.text} tone={status.tone} disabled={busy} onPower={power} />
      {notice && (
        <div className="notice" role="alert">
          {notice}
        </div>
      )}
      {snapshot.update.stage === "available" && (
        <div className="banner" role="status">
          <CircleArrowUp size={14} />
          <span className="banner-text">NetBraid {snapshot.update.latest} is ready to install.</span>
          <button type="button" className="link-btn" onClick={() => onOpen("settings")}>
            See what's new
          </button>
        </div>
      )}

      {snapshot.on ? (
        <Throughput snapshot={snapshot} colors={colors} />
      ) : (
        <section className="callout">
          <span className="callout-title">Add your networks together</span>
          <p className="callout-body">
            Each new connection goes out on the network with the most room. Browsers, download managers, Steam
            and torrents get the combined speed.
          </p>
          <button type="button" className="btn-primary" disabled={busy} onClick={() => power(true)}>
            <ShieldCheck size={14} />
            Turn on
          </button>
        </section>
      )}

      <div className="divider" />
      <section className="networks" aria-label="Networks">
        <div className="section-head">
          <span className="section-title">Networks</span>
          <span className="muted">
            {inUse(snapshot.links).length} of {snapshot.links.length} in use
          </span>
        </div>
        <div className="net-list">
          {shown.map((link) => (
            <NetworkCard
              key={link.id}
              link={link}
              color={colors.get(link.id) ?? "var(--text-3)"}
              live={snapshot.on}
              open={openId === link.id}
              onToggle={() => setOpenId(openId === link.id ? null : link.id)}
              run={run}
            />
          ))}
          {shown.length === 0 && (
            <div className="empty">{snapshot.links.length === 0 ? "No networks found." : "No network is connected."}</div>
          )}
        </div>
      </section>

      <Footer text={footerText(snapshot)} onOpen={onOpen} />
    </>
  );
}

function Throughput({ snapshot, colors }: { snapshot: Snapshot; colors: Map<string, string> }) {
  const links = inUse(snapshot.links);
  const [value, unit] = splitRate(useEased(snapshot.rxRate));
  const peak = Math.max(...totals(links.map((l) => window60(l.history))));

  return (
    <section className="hero" aria-label="Speed">
      <div className="hero-top">
        <div className="total">
          <span className="total-num">{value}</span>
          <span className="total-unit">{unit}</span>
        </div>
        <div className="hero-side">
          <span className="up num">
            <ArrowUp size={12} />
            {rate(snapshot.txRate)}
          </span>
          <span className="muted">Peak {rate(peak)}</span>
        </div>
      </div>
      <StreamChart links={links} colors={colors} />
      <div className="axis">
        <span>A minute ago</span>
        <span>Now</span>
      </div>
    </section>
  );
}
