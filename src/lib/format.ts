const UNITS = ["B", "KB", "MB", "GB", "TB"];

/** A byte count in decimal units, split so the number can be styled apart. */
export function splitBytes(n: number): [string, string] {
  let value = Math.max(0, n);
  let unit = 0;
  while (value >= 1000 && unit < UNITS.length - 1) {
    value /= 1000;
    unit++;
  }
  let text = value.toFixed(unit === 0 || value >= 100 ? 0 : 1);
  if (Number(text) >= 1000 && unit < UNITS.length - 1) {
    text = "1.0";
    unit++;
  }
  return [text, UNITS[unit]];
}

export const bytes = (n: number) => splitBytes(n).join(" ");

export function splitRate(n: number): [string, string] {
  const [value, unit] = splitBytes(n);
  return [value, `${unit}/s`];
}

export const rate = (n: number) => splitRate(n).join(" ");

export function count(n: number, one: string, many = `${one}s`): string {
  return `${n.toLocaleString("en-US")} ${n === 1 ? one : many}`;
}

export function age(seconds: number): string {
  if (seconds < 60) return `${seconds} s`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)} min`;
  return `${Math.floor(seconds / 3600)} h`;
}
