import type { LucideIcon } from "lucide-react";

interface Props {
  icon: LucideIcon;
  label: string;
  size?: number;
  onClick: () => void;
}

export function IconButton({ icon: Icon, label, size = 16, onClick }: Props) {
  return (
    <button type="button" className="icon-btn" aria-label={label} title={label} onClick={onClick}>
      <Icon size={size} />
    </button>
  );
}
