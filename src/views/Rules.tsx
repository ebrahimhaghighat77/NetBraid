import { useEffect, useState } from "react";
import { Ban, Trash2 } from "lucide-react";
import { api, message } from "../core/api";
import { BLOCK, type Rule, type Snapshot } from "../core/types";
import { SubHeader } from "../components/Header";
import { IconButton } from "../components/IconButton";
import { linkColors } from "../lib/colors";

interface Props {
  snapshot: Snapshot;
  onSnapshot: (next: Snapshot) => void;
  onBack: () => void;
}

function subject(rule: Rule): string {
  if (rule.app && rule.host) return `${rule.app} to ${rule.host}`;
  return rule.app || rule.host;
}

export function Rules({ snapshot, onSnapshot, onBack }: Props) {
  const rules = snapshot.settings.rules;
  const colors = linkColors(snapshot.links);
  const [app, setApp] = useState("");
  const [host, setHost] = useState("");
  const [link, setLink] = useState(snapshot.links.find((l) => l.present)?.id ?? BLOCK);
  const [fallback, setFallback] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [running, setRunning] = useState<string[]>([]);

  useEffect(() => {
    if (!snapshot.on) return;
    let alive = true;
    api
      .flows()
      .then((list) => {
        const names = new Set(list.map((f) => f.app).filter((a): a is string => !!a));
        if (alive) setRunning([...names].sort((a, b) => a.localeCompare(b)));
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [snapshot.on]);

  const save = (next: Rule[]) => {
    setError(null);
    return api
      .rulesSave(next)
      .then(onSnapshot)
      .catch((e) => setError(message(e)));
  };

  const add = () => {
    if (!app.trim() && !host.trim()) {
      setError("Name an app, a site, or both.");
      return;
    }
    void save([...rules, { app, host, link, fallback }]).then(() => {
      setApp("");
      setHost("");
    });
  };

  const where = (rule: Rule) => {
    if (rule.link === BLOCK) return "Blocked";
    const name = snapshot.links.find((l) => l.id === rule.link)?.name ?? "A network that is gone";
    return rule.fallback ? `${name}, or another while it's unavailable` : `${name} only`;
  };

  return (
    <>
      <SubHeader title="Rules" onBack={onBack} />
      {error && (
        <div className="notice" role="alert">
          {error}
        </div>
      )}
      <div className="divider" />
      <div className="page">
        {rules.length === 0 ? (
          <div className="empty">No rules yet. Add one to keep a game on Ethernet, or a bank on a single network.</div>
        ) : (
          <div className="rules" role="list">
            {rules.map((rule, i) => (
              <div key={`${rule.app}|${rule.host}|${i}`} className="rule" role="listitem">
                <div className="rule-text">
                  <span className="rule-what">{subject(rule)}</span>
                  <span className="rule-where">
                    {rule.link === BLOCK ? (
                      <Ban size={11} />
                    ) : (
                      <span className="dot" style={{ background: colors.get(rule.link) ?? "var(--text-3)" }} />
                    )}
                    {where(rule)}
                  </span>
                </div>
                <IconButton
                  icon={Trash2}
                  label={`Remove the rule for ${subject(rule)}`}
                  onClick={() => void save(rules.filter((_, at) => at !== i))}
                />
              </div>
            ))}
          </div>
        )}

        <form
          className="rule-form"
          onSubmit={(e) => {
            e.preventDefault();
            add();
          }}
        >
          <span className="section-title">New rule</span>
          <div className="rule-row">
            <label>
              App
              <input
                className="input"
                list="running-apps"
                placeholder="chrome.exe"
                spellCheck={false}
                value={app}
                onChange={(e) => setApp(e.target.value)}
              />
            </label>
            <label>
              Site
              <input
                className="input"
                placeholder="example.com"
                spellCheck={false}
                value={host}
                onChange={(e) => setHost(e.target.value)}
              />
            </label>
          </div>
          <datalist id="running-apps">
            {running.map((name) => (
              <option key={name} value={name} />
            ))}
          </datalist>
          <div className="rule-row">
            <label>
              Send through
              <select className="select" value={link} onChange={(e) => setLink(e.target.value)}>
                {snapshot.links.map((l) => (
                  <option key={l.id} value={l.id}>
                    {l.name}
                  </option>
                ))}
                <option value={BLOCK}>Nothing (block it)</option>
              </select>
            </label>
            <button type="submit" className="btn-primary">
              Add rule
            </button>
          </div>
          {link !== BLOCK && (
            <label className="check">
              <input type="checkbox" checked={fallback} onChange={(e) => setFallback(e.target.checked)} />
              Use another network while that one is unavailable
            </label>
          )}
          <span className="muted">Leave App or Site empty to match all of them. The first rule that fits wins.</span>
        </form>
      </div>
    </>
  );
}
