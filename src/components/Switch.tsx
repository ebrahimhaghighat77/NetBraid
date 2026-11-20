interface Props {
  on: boolean;
  label: string;
  disabled?: boolean;
  onChange: (on: boolean) => void;
}

export function Switch({ on, label, disabled, onChange }: Props) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      aria-label={label}
      className="switch"
      data-on={on}
      disabled={disabled}
      onClick={() => onChange(!on)}
    >
      <span className="switch-knob" />
    </button>
  );
}
