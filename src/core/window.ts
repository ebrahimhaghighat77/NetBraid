import { useEffect, useRef } from "react";
import { api } from "./api";

/** Keeps the window as tall as the popup's content. */
export function useFitHeight<T extends HTMLElement>() {
  const ref = useRef<T>(null);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    let sent = 0;
    const observer = new ResizeObserver(() => {
      const height = Math.ceil(el.getBoundingClientRect().height);
      if (height !== sent) {
        sent = height;
        void api.fit(height);
      }
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, []);
  return ref;
}
