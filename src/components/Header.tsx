import type { ReactNode } from "react";
import { ChevronLeft, Minus } from "lucide-react";
import { api } from "../core/api";
import type { Tone } from "../lib/summary";
import { IconButton } from "./IconButton";
import { Logo } from "./Logo";
import { Switch } from "./Switch";

interface Props {
  on: boolean;
  status: string;
  tone: Tone;
  disabled: boolean;
  onPower: (on: boolean) => void;
}

export function Header({ on, status, tone, disabled, onPower }: Props) {
  return (
    <header className="header">
      <Logo size={30} lit={on} />
      <div className="title">
        <span className="app-name">NetBraid</span>
        <span className="status" role="status">
          <span className="dot" data-tone={tone} />
          <span>{status}</span>
        </span>
      </div>
      <Switch on={on} label="Turn NetBraid on" disabled={disabled} onChange={onPower} />
      <IconButton icon={Minus} label="Hide" onClick={() => void api.hide()} />
    </header>
  );
}

export function SubHeader({ title, onBack, children }: { title: string; onBack: () => void; children?: ReactNode }) {
  return (
    <header className="header sub">
      <IconButton icon={ChevronLeft} label="Back" size={18} onClick={onBack} />
      <span className="header-title">{title}</span>
      {children}
      <span className="spacer" />
      <IconButton icon={Minus} label="Hide" onClick={() => void api.hide()} />
    </header>
  );
}
