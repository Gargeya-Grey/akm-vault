import type { Theme, View } from "../types";

const sections: { id: View; label: string }[] = [
  { id: "keys", label: "Keys" },
  { id: "activity", label: "Activity" },
  { id: "settings", label: "Settings" },
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
  const next = theme === "dark" ? "light" : "dark";

  return (
    <header className="navbar">
      <p className="bar-name">AKM</p>
      {unlocked && (
        <nav aria-label="Sections">
          <ul>
            {sections.map((section) => (
              <li key={section.id}>
                <button
                  type="button"
                  className="nav-link"
                  aria-current={view === section.id ? "page" : undefined}
                  onClick={() => onView(section.id)}
                >
                  {section.label}
                </button>
              </li>
            ))}
          </ul>
        </nav>
      )}
      {unlocked && <>
        <button type="button" className="text-button" onClick={onReload}>Reload vault</button>
        <button type="button" className="btn-secondary" onClick={onLock}>Lock now</button>
      </>}
      <button
        type="button"
        className="theme"
        aria-label={`Switch to ${next} mode`}
        onClick={onToggleTheme}
      >
        {next === "dark" ? "Dark mode" : "Light mode"}
      </button>
    </header>
  );
}
