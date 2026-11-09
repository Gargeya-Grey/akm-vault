import { useCallback, useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { Icon } from "./Icon";

interface Option {
  value: string;
  label: string;
}

interface Props {
  id: string;
  label: string;
  value: string;
  placeholder: string;
  options: Option[];
  onChange: (value: string) => void;
  disabled?: boolean;
}

interface Placement {
  left: number;
  width: number;
  top?: number;
  bottom?: number;
  maxHeight: number;
  up: boolean;
}

const GAP = 6;
const EDGE = 12;
const OPEN_KEYS = new Set([" ", "Enter", "F4", "ArrowDown", "ArrowUp"]);

function placeBelow(trigger: DOMRect): Placement {
  const below = window.innerHeight - trigger.bottom - GAP - EDGE;
  const above = trigger.top - GAP - EDGE;
  const up = below < 220 && above > below;
  const room = Math.min(320, up ? above : below);
  return {
    left: trigger.left,
    width: trigger.width,
    maxHeight: Math.max(120, room),
    up,
    ...(up ? { bottom: window.innerHeight - trigger.top + GAP } : { top: trigger.bottom + GAP }),
  };
}

/// A labelled select. The real select stays as the field, so its name, value and
/// automation stay the same. A real press or a key opens this list in place of the
/// system's list. Focus stays on the field. The list closes when the field loses focus.
/// The highlight is an option's value, not its position, because the options can change
/// while the list is open. Only opening the list and a keyboard move scroll it.
export function FilterSelect({
  id,
  label,
  value,
  placeholder,
  options,
  onChange,
  disabled = false,
}: Props) {
  const field = useRef<HTMLSelectElement>(null);
  const typed = useRef({ text: "", at: 0 });
  const menuId = useId();
  const [open, setOpen] = useState(false);
  const [activeValue, setActiveValue] = useState("");
  const [revealRequest, setRevealRequest] = useState(0);
  const [place, setPlace] = useState<Placement | null>(null);

  const choices: Option[] = [{ value: "", label: placeholder }, ...options];
  const current = Math.max(0, choices.findIndex((choice) => choice.value === value));
  const highlighted = choices.findIndex((choice) => choice.value === activeValue);
  const active = highlighted >= 0 ? highlighted : current;
  // A highlighted option that a refresh removed stays reset, even if the option returns.
  if (highlighted < 0 && activeValue !== choices[current].value) setActiveValue(choices[current].value);
  const optionId = (index: number) => `${menuId}-${index}`;
  const showing = open && !disabled;

  const position = useCallback(() => {
    if (field.current) setPlace(placeBelow(field.current.getBoundingClientRect()));
  }, []);

  useLayoutEffect(() => {
    if (!showing) return;
    position();
    window.addEventListener("resize", position);
    window.addEventListener("scroll", position, true);
    return () => {
      window.removeEventListener("resize", position);
      window.removeEventListener("scroll", position, true);
    };
  }, [showing, position]);

  const placed = showing && place !== null;
  useLayoutEffect(() => {
    if (placed) document.getElementById(optionId(active))?.scrollIntoView?.({ block: "nearest" });
    // Only a new request or a newly placed list scrolls. A refresh, a hover and the wheel do not.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [revealRequest, placed]);

  useEffect(() => {
    if (disabled) setOpen(false);
  }, [disabled]);

  function show() {
    setActiveValue(choices[current].value);
    setPlace(null);
    setOpen(true);
  }

  function moveTo(index: number) {
    setActiveValue(choices[index].value);
    setRevealRequest((request) => request + 1);
  }

  function choose(next: Option) {
    setOpen(false);
    if (next.value !== value) onChange(next.value);
  }

  function matchTyped(key: string) {
    const now = Date.now();
    const text = now - typed.current.at > 500 ? key : typed.current.text + key;
    typed.current = { text, at: now };
    const wanted = text.toLocaleLowerCase();
    const found = choices.findIndex((choice) => choice.label.toLocaleLowerCase().startsWith(wanted));
    if (found >= 0) moveTo(found);
  }

  function onKeyDown(event: React.KeyboardEvent<HTMLSelectElement>) {
    if (!showing) {
      if (OPEN_KEYS.has(event.key)) {
        event.preventDefault();
        show();
      }
      return;
    }
    const last = choices.length - 1;
    const moves: Record<string, number> = {
      ArrowDown: Math.min(last, active + 1),
      ArrowUp: Math.max(0, active - 1),
      Home: 0,
      End: last,
      PageDown: Math.min(last, active + 8),
      PageUp: Math.max(0, active - 8),
    };
    if (event.key in moves) {
      event.preventDefault();
      moveTo(moves[event.key]);
    } else if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      choose(choices[active]);
    } else if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      setOpen(false);
    } else if (event.key === "Tab") {
      setOpen(false);
    } else if (event.key.length === 1 && !event.ctrlKey && !event.altKey && !event.metaKey) {
      event.preventDefault();
      matchTyped(event.key);
    }
  }

  return (
    <div className="filter">
      <label className="label" htmlFor={id}>{label}</label>
      <select
        ref={field}
        id={id}
        value={value}
        disabled={disabled}
        aria-expanded={showing || undefined}
        aria-controls={showing ? menuId : undefined}
        aria-activedescendant={showing ? optionId(active) : undefined}
        onChange={(event) => onChange(event.target.value)}
        onMouseDown={(event) => {
          // A press from a screen reader or UI Automation has no click count. It gets the system's list, which those tools can drive.
          if (event.detail === 0) return;
          event.preventDefault();
          if (disabled) return;
          field.current?.focus();
          if (showing) setOpen(false);
          else show();
        }}
        onKeyDown={onKeyDown}
        onBlur={() => setOpen(false)}
      >
        {choices.map((choice) => (
          <option key={choice.value} value={choice.value}>
            {choice.label}
          </option>
        ))}
      </select>
      {showing && place && createPortal(
        <ul
          id={menuId}
          role="listbox"
          aria-label={label}
          className={`select-menu${place.up ? " select-menu-up" : ""}`}
          style={{ left: place.left, width: place.width, top: place.top, bottom: place.bottom, maxHeight: place.maxHeight }}
          onMouseDown={(event) => event.preventDefault()}
        >
          {choices.map((choice, index) => (
            <li
              key={choice.value}
              id={optionId(index)}
              role="option"
              aria-selected={index === current}
              className={`select-option${index === active ? " active" : ""}`}
              onMouseMove={() => setActiveValue(choice.value)}
              onClick={() => choose(choice)}
            >
              <span>{choice.label}</span>
              {index === current && <Icon name="check" />}
            </li>
          ))}
        </ul>,
        document.body,
      )}
    </div>
  );
}

export function uniqueOptions(values: (string | null)[]): Option[] {
  const seen = new Set<string>();
  for (const value of values) {
    if (value) seen.add(value);
  }
  return [...seen]
    .sort((a, b) => a.localeCompare(b))
    .map((value) => ({ value, label: value }));
}
