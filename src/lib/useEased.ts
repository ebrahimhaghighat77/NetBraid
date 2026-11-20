import { useEffect, useRef, useState } from "react";

/** Glides toward each new value, so once-a-second rates roll instead of jumping. */
export function useEased(target: number, ms = 450): number {
  const [shown, setShown] = useState(target);
  const current = useRef(target);

  useEffect(() => {
    const from = current.current;
    const started = performance.now();
    let frame = requestAnimationFrame(function step(now) {
      const t = Math.min(1, (now - started) / ms);
      const value = from + (target - from) * (1 - (1 - t) ** 3);
      current.current = value;
      setShown(value);
      if (t < 1) frame = requestAnimationFrame(step);
    });
    return () => cancelAnimationFrame(frame);
  }, [target, ms]);

  return shown;
}
