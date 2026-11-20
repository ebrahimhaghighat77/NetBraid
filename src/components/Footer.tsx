import { List, Route, ScrollText, Settings } from "lucide-react";
import type { Page } from "../App";
import { IconButton } from "./IconButton";

export function Footer({ text, onOpen }: { text: string; onOpen: (page: Page) => void }) {
  return (
    <footer className="footer">
      <span className="footer-info">{text}</span>
      <div className="footer-actions">
        <IconButton icon={List} label="Connections" onClick={() => onOpen("activity")} />
        <IconButton icon={Route} label="Rules for apps and sites" onClick={() => onOpen("rules")} />
        <IconButton icon={ScrollText} label="Log" onClick={() => onOpen("log")} />
        <IconButton icon={Settings} label="Settings" onClick={() => onOpen("settings")} />
      </div>
    </footer>
  );
}
