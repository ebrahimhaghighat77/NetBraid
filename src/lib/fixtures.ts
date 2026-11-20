import type { LinkView, Snapshot } from "../core/types";

export function sampleLink(change: Partial<LinkView> = {}): LinkView {
  return {
    id: "{wifi}",
    name: "Wi-Fi",
    description: "Wireless adapter",
    kind: "wifi",
    address: "192.168.1.36",
    gateway: "192.168.1.1",
    latencyMs: 40,
    reach: "full",
    present: true,
    reason: "",
    virtualAdapter: false,
    isDefault: false,
    enabled: true,
    weight: 5,
    backup: false,
    capMb: null,
    capPeriod: "month",
    usedBytes: 0,
    capped: false,
    rxRate: 0,
    txRate: 0,
    share: 0,
    connections: 0,
    history: [],
    ...change,
  };
}

export function sampleSnapshot(change: Partial<Snapshot> = {}): Snapshot {
  return {
    version: "0.1.0",
    on: false,
    busy: false,
    error: null,
    rxRate: 0,
    txRate: 0,
    connections: 0,
    todayBytes: 0,
    links: [],
    settings: {
      adaptive: true,
      sticky: false,
      dns: ["1.1.1.1", "8.8.8.8"],
      rules: [],
      hideUnavailable: false,
      checkUpdates: true,
      autostart: false,
    },
    update: { stage: "idle", latest: null, notes: null, progress: null, error: null },
    ...change,
  };
}
