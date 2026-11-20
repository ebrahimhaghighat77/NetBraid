import { useEffect, useState } from "react";
import { api } from "./core/api";
import { useSnapshot } from "./core/useSnapshot";
import { useFitHeight } from "./core/window";
import { Activity } from "./views/Activity";
import { Home } from "./views/Home";
import { Log } from "./views/Log";
import { Rules } from "./views/Rules";
import { Settings } from "./views/Settings";

export type Page = "home" | "settings" | "activity" | "rules" | "log";

export default function App() {
  const ref = useFitHeight<HTMLDivElement>();
  const [snapshot, setSnapshot] = useSnapshot();
  const [page, setPage] = useState<Page>("home");
  const [homeHeight, setHomeHeight] = useState<number | null>(null);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      if (page === "home") void api.hide();
      else setPage("home");
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [page]);

  const open = (next: Page) => {
    if (page === "home" && ref.current) setHomeHeight(Math.ceil(ref.current.getBoundingClientRect().height));
    setPage(next);
  };
  const back = () => setPage("home");
  // Settings is long; it keeps the home page's height and scrolls.
  const fixed = page === "settings" && homeHeight ? { height: homeHeight } : undefined;

  const content = () => {
    if (!snapshot) return <div className="empty">Starting…</div>;
    switch (page) {
      case "settings":
        return <Settings snapshot={snapshot} onSnapshot={setSnapshot} onBack={back} />;
      case "activity":
        return <Activity snapshot={snapshot} onBack={back} />;
      case "rules":
        return <Rules snapshot={snapshot} onSnapshot={setSnapshot} onBack={back} />;
      case "log":
        return <Log onBack={back} />;
      default:
        return <Home snapshot={snapshot} onSnapshot={setSnapshot} onOpen={open} />;
    }
  };

  return (
    <div ref={ref} className="popup" style={fixed}>
      {content()}
    </div>
  );
}
