import { useEffect, useState } from "react";
import { api } from "../core/api";
import { SubHeader } from "../components/Header";

const POLL_MS = 2000;

function level(line: string): string | undefined {
  if (line.includes(" ERROR ")) return "error";
  if (line.includes(" WARN ")) return "warn";
  return undefined;
}

export function Log({ onBack }: { onBack: () => void }) {
  const [lines, setLines] = useState<string[]>([]);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    let alive = true;
    const load = () =>
      api
        .log()
        .then((list) => alive && setLines(list))
        .catch(() => {});
    load();
    const timer = window.setInterval(load, POLL_MS);
    return () => {
      alive = false;
      window.clearInterval(timer);
    };
  }, []);

  const copy = () => {
    void navigator.clipboard.writeText(lines.join("\n")).then(() => {
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1500);
    });
  };

  return (
    <>
      <SubHeader title="Log" onBack={onBack} />
      <div className="divider" />
      <div className="page">
        {lines.length === 0 ? (
          <div className="empty">Nothing logged yet.</div>
        ) : (
          <pre className="log">
            {[...lines].reverse().map((line, i) => (
              <span key={lines.length - i} className={level(line)}>
                {line}
                {"\n"}
              </span>
            ))}
          </pre>
        )}
      </div>
      <div className="log-foot">
        <span className="muted">Newest first, times in UTC. The full log is in %APPDATA%\ir.scidev.netbraid\logs.</span>
        <button type="button" className="btn" disabled={lines.length === 0} onClick={copy}>
          {copied ? "Copied" : "Copy"}
        </button>
      </div>
    </>
  );
}
