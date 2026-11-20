// Mirrors src-tauri/src/snapshot.rs and commands.rs.

export type Kind = "ethernet" | "wifi" | "phone" | "cellular" | "other";
export type Reach = "unknown" | "full" | "local" | "down";
export type Period = "day" | "month";
export type Stage = "idle" | "checking" | "current" | "available" | "downloading" | "failed";

export interface LinkView {
  id: string;
  name: string;
  description: string;
  kind: Kind;
  address: string | null;
  gateway: string | null;
  latencyMs: number | null;
  reach: Reach;
  present: boolean;
  reason: string;
  virtualAdapter: boolean;
  isDefault: boolean;
  enabled: boolean;
  weight: number;
  backup: boolean;
  capMb: number | null;
  capPeriod: Period;
  usedBytes: number;
  capped: boolean;
  rxRate: number;
  txRate: number;
  share: number;
  connections: number;
  history: number[];
}

export interface Rule {
  app: string;
  host: string;
  link: string;
  fallback: boolean;
}

export const BLOCK = "block";

export interface SettingsView {
  adaptive: boolean;
  sticky: boolean;
  dns: string[];
  rules: Rule[];
  hideUnavailable: boolean;
  checkUpdates: boolean;
  autostart: boolean;
}

export interface UpdateView {
  stage: Stage;
  latest: string | null;
  notes: string | null;
  progress: number | null;
  error: string | null;
}

export interface Snapshot {
  version: string;
  on: boolean;
  busy: boolean;
  error: string | null;
  rxRate: number;
  txRate: number;
  connections: number;
  todayBytes: number;
  links: LinkView[];
  settings: SettingsView;
  update: UpdateView;
}

export interface FlowView {
  id: number;
  udp: boolean;
  app: string | null;
  host: string | null;
  destination: string;
  linkId: string | null;
  linkName: string | null;
  rx: number;
  tx: number;
  rxRate: number;
  txRate: number;
  seconds: number;
}

export interface LinkChange {
  enabled?: boolean;
  weight?: number;
  backup?: boolean;
}

export interface OptionChange {
  adaptive?: boolean;
  sticky?: boolean;
  dns?: string[];
  hideUnavailable?: boolean;
  checkUpdates?: boolean;
  autostart?: boolean;
}

export type LinkTarget = "github" | "scidev" | "source" | "releases";
