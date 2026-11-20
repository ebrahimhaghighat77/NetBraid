import { useEffect, useState } from "react";
import { api } from "../core/api";
import type { FlowView, Snapshot } from "../core/types";
import { SubHeader } from "../components/Header";
import { linkColors } from "../lib/colors";
import { age, count, rate } from "../lib/format";

const POLL_MS = 1000;

export function Activity({ snapshot, onBack }: { snapshot: Snapshot; onBack: () => void }) {
  const [flows, setFlows] = useState<FlowView[]>([]);

  useEffect(() => {
    if (!snapshot.on) {
      setFlows([]);
      return;
    }
    let alive = true;
    const load = () =>
      api
        .flows()
        .then((list) => alive && setFlows(list))
        .catch(() => {});
    load();
    const timer = window.setInterval(load, POLL_MS);
    return () => {
      alive = false;
      window.clearInterval(timer);
    };
  }, [snapshot.on]);

  const colors = linkColors(snapshot.links);

  return (
    <>
      <SubHeader title="Connections" onBack={onBack}>
        {snapshot.on && <span className="chip">{count(flows.length, "open", "open")}</span>}
      </SubHeader>
      <div className="divider" />
      {flows.length === 0 ? (
        <div className="empty">
          {snapshot.on ? "Nothing is connected right now." : "Turn NetBraid on to see which network each connection uses."}
        </div>
      ) : (
        <div className="page flows" role="list">
          {flows.map((f) => (
            <div key={f.id} className="flow" role="listitem">
              <div className="flow-who">
                <span className="flow-app">{f.app ?? (f.udp ? "UDP" : "Unknown app")}</span>
                <span className="muted" title={f.destination}>
                  {f.host ?? f.destination}
                </span>
              </div>
              <div className="flow-where">
                <span className="num">{f.rxRate + f.txRate > 0 ? rate(f.rxRate + f.txRate) : age(f.seconds)}</span>
                <span className="flow-link muted">
                  <span className="dot" style={{ background: (f.linkId && colors.get(f.linkId)) || "var(--text-3)" }} />
                  {f.linkName ?? "Connecting"}
                </span>
              </div>
            </div>
          ))}
        </div>
      )}
    </>
  );
}
