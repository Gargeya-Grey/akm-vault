import { useLayoutEffect, useRef, useState } from "react";
import type { Theme, View } from "../types";
import { BrandMark } from "./glyphs";
import { Icon } from "./Icon";
import type { IconName } from "./Icon";

const primary: { id: View; label: string; icon: IconName }[] = [
  { id: "keys", label: "Keys", icon: "key" },
  { id: "use", label: "Use keys", icon: "plug" },
  { id: "activity", label: "Activity", icon: "pulse" },
];

const secondary: { id: View; label: string; icon: IconName }[] = [
  { id: "bin", label: "Recycle Bin", icon: "trash" },
  { id: "settings", label: "Settings", icon: "sliders" },
];

interface Props {
  view: View;
  onView: (view: View) => void;
  unlocked: boolean;
  theme: Theme;
  onToggleTheme: () => void;
  onLock: () => void;
  onReload: () => void;
}

export function NavBar({ view, onView, unlocked, theme, onToggleTheme, onLock, onReload }: Props) {
  const navRef = useRef<HTMLElement>(null);
  const [pillTop, setPillTop] = useState<number | null>(null);

  useLayoutEffect(() => {
    if (!unlocked) return;
    const place = () => {
      const nav = navRef.current;
      const current = nav?.querySelector('[aria-current="page"]');
      setPillTop(nav && current ? current.getBoundingClientRect().top - nav.getBoundingClientRect().top : null);
    };
    place();
    window.addEventListener("resize", place);
    return () => window.removeEventListener("resize", place);
  }, [view, unlocked]);

  const next = theme === "dark" ? "light" : "dark";
  const themeSwitch = (
    <button type="button" className="theme" aria-label={`Switch to ${next} mode`} onClick={onToggleTheme}>
      <Icon name={next === "dark" ? "moon" : "sun"} size={18} />
      <span>{next === "dark" ? "Dark mode" : "Light mode"}</span>
    </button>
  );

  if (!unlocked) return <header className="lock-corner">{themeSwitch}</header>;

  const link = (section: { id: View; label: string; icon: IconName }) => (
    <li key={section.id}>
      <button type="button" className="nav-link" aria-current={view === section.id ? "page" : undefined} onClick={() => onView(section.id)}>
        <Icon name={section.icon} size={18} />
        <span>{section.label}</span>
      </button>
    </li>
  );

  return (
    <header className="rail">
      <div className="rail-brand">
        <BrandMark />
        <p className="bar-name">AKM</p>
      </div>
      <nav ref={navRef} aria-label="Sections">
        {pillTop !== null && <span className="nav-pill" aria-hidden="true" style={{ transform: `translateY(${pillTop}px)` }} />}
        <ul>{primary.map(link)}</ul>
        <ul className="rail-lower">{secondary.map(link)}</ul>
      </nav>
      <div className="rail-tools">
        <button type="button" className="rail-tool" onClick={onReload}>
          <Icon name="refresh" size={18} />
          <span>Reload vault</span>
        </button>
        {themeSwitch}
        <button type="button" className="rail-lock" onClick={onLock}>
          <Icon name="lock" size={18} />
          <span>Lock now</span>
        </button>
      </div>
    </header>
  );
}
