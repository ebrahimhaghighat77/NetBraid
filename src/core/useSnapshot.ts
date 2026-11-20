import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "./api";
import type { Snapshot } from "./types";

export function useSnapshot(): [Snapshot | null, (next: Snapshot) => void] {
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);

  useEffect(() => {
    let alive = true;
    const load = () =>
      api
        .snapshot()
        .then((s) => alive && setSnapshot(s))
        .catch(() => {});
    load();
    const stop = listen<Snapshot>("snapshot", (e) => alive && setSnapshot(e.payload));
    window.addEventListener("focus", load);
    return () => {
      alive = false;
      void stop.then((unlisten) => unlisten());
      window.removeEventListener("focus", load);
    };
  }, []);

  return [snapshot, setSnapshot];
}
