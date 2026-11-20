// Stands in for the Rust side when the popup runs in a plain browser (`npm run dev`).
import type { FlowView, LinkView, Rule, Snapshot } from "../core/types";
import { sampleLink, sampleSnapshot } from "../lib/fixtures";

type Args = Record<string, unknown>;
type Callback = (message: unknown) => void;

const callbacks = new Map<number, Callback>();
const listeners = new Set<number>();
let nextId = 1;
let t = 0;
let on = true;
let rules: Rule[] = [
  { app: "valorant.exe", host: "", link: "{eth}", fallback: false },
  { app: "", host: "bank.example.com", link: "{wifi}", fallback: true },
];
const settings = { ...sampleSnapshot().settings, autostart: true };
const update = { ...sampleSnapshot().update };

const links: LinkView[] = [
  sampleLink({ id: "{eth}", name: "Ethernet", kind: "ethernet", description: "Intel(R) Ethernet Controller I225-V", address: "192.168.1.20", latencyMs: 31, isDefault: true, weight: 7 }),
  sampleLink({ id: "{wifi}", name: "Wi-Fi", kind: "wifi", description: "Intel(R) Wi-Fi 6 AX201", address: "10.0.0.37", latencyMs: 44 }),
  sampleLink({ id: "{phone}", name: "Phone", kind: "phone", description: "Remote NDIS based Internet Sharing Device", address: "192.168.42.17", latencyMs: 58, capMb: 20_000, usedBytes: 12_400_000_000 }),
  sampleLink({ id: "{lte}", name: "Cellular", kind: "cellular", description: "Mobile Broadband Adapter", address: null, present: false, reason: "not connected", latencyMs: null }),
];

const wave = (base: number, size: number, speed: number, phase: number) =>
  Math.max(0, base + size * Math.sin(t * speed + phase) + size * 0.5 * Math.sin(t * speed * 3.1));

function tick() {
  t += 1;
  const rates = [wave(2.9e6, 1.3e6, 0.21, 0), wave(1.7e6, 0.9e6, 0.33, 2), wave(0.9e6, 0.6e6, 0.17, 4)];
  links.forEach((l, i) => {
    const active = on && l.present && l.enabled && !l.capped && !(l.backup && i > 0);
    l.rxRate = active ? Math.round(rates[i] ?? 0) : 0;
    l.txRate = Math.round(l.rxRate * 0.06);
    l.connections = active ? 12 + ((i * 7 + t) % 9) : 0;
    l.history = [...l.history.slice(-59), l.rxRate];
    l.usedBytes += l.rxRate + l.txRate;
    l.capped = l.capMb != null && l.usedBytes >= l.capMb * 1e6;
  });
  const total = links.reduce((sum, l) => sum + l.rxRate, 0);
  links.forEach((l) => (l.share = total ? Math.round((l.rxRate / total) * 100) : 0));
}

function snapshot(): Snapshot {
  return sampleSnapshot({
    on,
    rxRate: links.reduce((sum, l) => sum + l.rxRate, 0),
    txRate: links.reduce((sum, l) => sum + l.txRate, 0),
    connections: links.reduce((sum, l) => sum + l.connections, 0),
    todayBytes: 3_400_000_000 + t * 5e6,
    links: links.map((l) => ({ ...l })),
    settings: { ...settings, rules },
    update: { ...update },
  });
}

function flows(): FlowView[] {
  const apps = ["chrome.exe", "steam.exe", "Telegram.exe", "valorant.exe", "msedge.exe"];
  const hosts = ["youtube.com", "steamcontent.com", "web.telegram.org", null, "github.com"];
  return apps.map((app, i) => ({
    id: i,
    udp: i === 3,
    app,
    host: hosts[i],
    destination: `104.18.${i}.20:443`,
    linkId: links[i % 3].id,
    linkName: links[i % 3].name,
    rx: 1e6 * (i + 1),
    tx: 1e5,
    rxRate: on ? Math.round(wave(4e5, 3e5, 0.4, i)) : 0,
    txRate: 2e4,
    seconds: 40 * (i + 1),
  }));
}

const LOG = [
  "2026-11-20 08:12:01Z INFO  NetBraid 0.1.0 started",
  "2026-11-20 08:12:01Z INFO  network Ethernet (Ethernet) 192.168.1.20: available",
  "2026-11-20 08:12:02Z INFO  turned on (adapter 41)",
  "2026-11-20 08:14:40Z WARN  Wi-Fi has no internet: nothing answered",
  "2026-11-20 08:14:52Z INFO  Wi-Fi reaches the internet",
];

const find = (args: Args) => links.find((l) => l.id === args.id) ?? links[0];

function handle(cmd: string, args: Args = {}): unknown {
  switch (cmd) {
    case "plugin:event|listen":
      if (args.event === "snapshot") listeners.add(args.handler as number);
      return args.handler;
    case "power":
      on = args.on as boolean;
      break;
    case "link_update":
      Object.assign(find(args), args.change);
      break;
    case "link_cap":
      Object.assign(find(args), { capMb: args.capMb, capPeriod: args.period });
      break;
    case "usage_reset":
      find(args).usedBytes = 0;
      break;
    case "rules_save":
      rules = args.rules as Rule[];
      break;
    case "options_save":
      Object.assign(settings, args.change);
      break;
    case "update_check":
      Object.assign(update, { stage: "available", latest: "0.2.0", notes: "Faster failover when a network drops.\nA smaller installer." });
      break;
    case "log_lines":
      return LOG;
    case "flows_list":
      return flows();
    case "snapshot":
      break;
    default:
      return null;
  }
  return snapshot();
}

Object.assign(window, {
  __TAURI_INTERNALS__: {
    invoke: (cmd: string, args?: Args) => Promise.resolve(handle(cmd, args)),
    transformCallback: (callback: Callback) => {
      callbacks.set(nextId, callback);
      return nextId++;
    },
    unregisterCallback: (id: number) => callbacks.delete(id),
  },
  __TAURI_EVENT_PLUGIN_INTERNALS__: {
    unregisterListener: (_event: string, id: number) => listeners.delete(id),
  },
});

for (let i = 0; i < 60; i++) tick();
window.setInterval(() => {
  tick();
  for (const id of listeners) callbacks.get(id)?.({ event: "snapshot", id, payload: snapshot() });
}, 1000);

document.documentElement.style.background = "linear-gradient(135deg, #2a3350, #1d1a2b)";
