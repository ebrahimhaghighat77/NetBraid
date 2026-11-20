import { useState, type KeyboardEvent, type ReactNode } from "react";
import { ChevronRight, CodeXml, Globe, UserRound } from "lucide-react";
import { api, message } from "../core/api";
import type { OptionChange, Snapshot } from "../core/types";
import { SubHeader } from "../components/Header";
import { Mark } from "../components/Mark";
import { Switch } from "../components/Switch";

interface Props {
  snapshot: Snapshot;
  onSnapshot: (next: Snapshot) => void;
  onBack: () => void;
}

export function Settings({ snapshot, onSnapshot, onBack }: Props) {
  const [error, setError] = useState<string | null>(null);
  const settings = snapshot.settings;

  const change = (option: OptionChange) => {
    setError(null);
    return api
      .optionsSave(option)
      .then(onSnapshot)
      .catch((e) => {
        setError(message(e));
        throw e;
      });
  };

  return (
    <>
      <SubHeader title="Settings" onBack={onBack} />
      {error && (
        <div className="notice" role="alert">
          {error}
        </div>
      )}
      <div className="divider" />
      <div className="settings-body">
        <Section title="Balancing">
          <Row
            label="Learn each network's speed"
            desc="Faster networks get more new connections, based on what they actually carry."
            on={settings.adaptive}
            onChange={(adaptive) => void change({ adaptive })}
          />
          <Row
            label="Keep each site on one network"
            desc="Stops sign-outs on banks and some games; a single site no longer gets the combined speed."
            on={settings.sticky}
            onChange={(sticky) => void change({ sticky })}
          />
        </Section>

        <Section title="App">
          <Row
            label="Start with Windows"
            desc="Opens quietly in the tray when you sign in."
            on={settings.autostart}
            onChange={(autostart) => void change({ autostart })}
          />
          <Row
            label="Hide unavailable networks"
            desc="Leaves unplugged cables and networks that aren't connected out of the list."
            on={settings.hideUnavailable}
            onChange={(hideUnavailable) => void change({ hideUnavailable })}
          />
          <Dns servers={settings.dns} onSave={(dns) => change({ dns })} />
        </Section>

        <Section title="Updates">
          <Updates snapshot={snapshot} onSnapshot={onSnapshot} />
          <Row
            label="Check when NetBraid starts"
            desc="Asks GitHub for a newer version. Nothing is installed until you say so."
            on={settings.checkUpdates}
            onChange={(checkUpdates) => void change({ checkUpdates })}
          />
        </Section>

        <Section title="About">
          <About version={snapshot.version} />
        </Section>
      </div>
    </>
  );
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="section" aria-label={title}>
      <span className="section-title">{title}</span>
      <div className="group">{children}</div>
    </section>
  );
}

function Row({ label, desc, on, onChange }: { label: string; desc?: string; on: boolean; onChange: (on: boolean) => void }) {
  return (
    <div className="row">
      <div className="row-text">
        <span>{label}</span>
        {desc && <span className="row-desc">{desc}</span>}
      </div>
      <Switch on={on} label={label} onChange={onChange} />
    </div>
  );
}

function Dns({ servers, onSave }: { servers: string[]; onSave: (dns: string[]) => Promise<unknown> }) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState("");

  const open = () => {
    setDraft(servers.join(", "));
    setEditing(true);
  };
  const save = () => {
    onSave(draft.split(/[\s,]+/).filter(Boolean))
      .then(() => setEditing(false))
      .catch(() => {});
  };
  const onKey = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter") save();
    if (e.key === "Escape") {
      e.stopPropagation();
      setEditing(false);
    }
  };

  if (!editing) {
    return (
      <button type="button" className="row" onClick={open}>
        <div className="row-text">
          <span>DNS servers</span>
          <span className="row-desc">{servers.join(", ")}</span>
        </div>
        <ChevronRight size={16} />
      </button>
    );
  }
  return (
    <div className="row">
      <div className="row-text">
        <span>DNS servers</span>
        <input
          className="input"
          value={draft}
          autoFocus
          spellCheck={false}
          aria-label="DNS servers"
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={onKey}
        />
        <span className="row-desc">Separate with commas, then press Enter. Lookups go through every network at once.</span>
      </div>
    </div>
  );
}

function Updates({ snapshot, onSnapshot }: { snapshot: Snapshot; onSnapshot: (next: Snapshot) => void }) {
  const { stage, latest, notes, progress, error } = snapshot.update;
  const [failure, setFailure] = useState<string | null>(null);

  const check = () => {
    setFailure(null);
    api.updateCheck().then(onSnapshot).catch((e) => setFailure(message(e)));
  };
  const install = () => {
    setFailure(null);
    api.updateInstall().catch((e) => setFailure(message(e)));
  };

  let title = `You have NetBraid ${snapshot.version}`;
  let desc = "Look for a newer version on GitHub.";
  if (stage === "checking") desc = "Checking for updates…";
  if (stage === "current") desc = "This is the newest version.";
  if (stage === "failed") desc = error ?? "The check failed.";
  if (stage === "available" || stage === "downloading") {
    title = `NetBraid ${latest} is available`;
    desc = stage === "downloading" ? `Downloading… ${progress ?? 0}%` : `You have ${snapshot.version}.`;
  }

  return (
    <div className="update">
      <div className="row-text">
        <span>{title}</span>
        <span className="row-desc">{failure ?? desc}</span>
      </div>
      {stage === "available" && notes && <p className="update-notes">{notes.slice(0, 400)}</p>}
      <div className="update-actions">
        {stage === "available" ? (
          <button type="button" className="btn-primary" onClick={install}>
            Download and install
          </button>
        ) : (
          <button type="button" className="btn" disabled={stage === "checking" || stage === "downloading"} onClick={check}>
            Check for updates
          </button>
        )}
        <button type="button" className="link-btn" onClick={() => void api.openLink("releases")}>
          All releases
        </button>
      </div>
    </div>
  );
}

function About({ version }: { version: string }) {
  return (
    <div className="about">
      <div className="about-head">
        <Mark size={34} />
        <div className="row-text">
          <span className="about-name">NetBraid {version}</span>
          <span className="row-desc">Made by SciDev. Free and open source under the MIT License.</span>
        </div>
      </div>
      <div className="about-links">
        <button type="button" className="about-link" onClick={() => void api.openLink("github")}>
          <UserRound size={14} />
          <span>
            <span className="about-link-name">GitHub</span>
            <span className="row-desc">ebrahimhaghighat77</span>
          </span>
        </button>
        <button type="button" className="about-link" onClick={() => void api.openLink("scidev")}>
          <Globe size={14} />
          <span>
            <span className="about-link-name">SciDev team</span>
            <span className="row-desc">scidev.ir</span>
          </span>
        </button>
        <button type="button" className="about-link" onClick={() => void api.openLink("source")}>
          <CodeXml size={14} />
          <span>
            <span className="about-link-name">Source code</span>
            <span className="row-desc">github.com/ebrahimhaghighat77/NetBraid</span>
          </span>
        </button>
      </div>
    </div>
  );
}
