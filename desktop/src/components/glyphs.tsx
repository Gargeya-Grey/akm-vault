import type { CSSProperties } from "react";

function hash(text: string): number {
  let value = 0x811c9dc5;
  for (let index = 0; index < text.length; index += 1) {
    value ^= text.charCodeAt(index);
    value = Math.imul(value, 0x01000193);
  }
  return value >>> 0;
}

/** The depth of each cut, from 0 to 4. It comes from the entry's ID and never from its secret. */
export function cutsOf(id: string, count = 7): number[] {
  return Array.from({ length: count }, (_, index) => hash(`${id}:${index}`) % 5);
}

/** The bow is the part people recognise first, so there are three shapes of it. */
const BOWS = [
  "M10 3a9 9 0 1 0 0 18a9 9 0 1 0 0-18z",
  "M5.5 2.5h9a3 3 0 0 1 3 3v13a3 3 0 0 1-3 3h-9a3 3 0 0 1-3-3v-13a3 3 0 0 1 3-3z",
  "M10 2l8 5v10l-8 5l-8-5V7z",
];
const BOW_HOLE = "M8 9.2a2.8 2.8 0 1 1 0 5.6a2.8 2.8 0 1 1 0-5.6z";

function bladePath(cuts: number[]): string {
  const shoulder = 17;
  const tip = 70;
  const top = 8;
  const spine = 13;
  const start = tip - 4;
  const end = shoulder + 5;
  const step = (start - end) / cuts.length;
  let path = `M${shoulder} ${top}H${start}L${tip} ${top + 3.5}L${start} ${spine}`;
  cuts.forEach((depth, index) => {
    const x = start - index * step;
    const y = spine + 1 + depth * 1.75;
    path += `L${(x - step * 0.18).toFixed(1)} ${y}L${(x - step * 0.82).toFixed(1)} ${y}L${(x - step).toFixed(1)} ${spine}`;
  });
  return `${path}H${shoulder}Z`;
}

/** A key drawn with its own cut. People find a key by its shape before they read its name. */
export function KeyCut({ id, tone = "ok", width = 58 }: { id: string; tone?: "ok" | "warn"; width?: number }) {
  return (
    <svg className={`keycut keycut-${tone}`} width={width} height={(width * 24) / 72} viewBox="0 0 72 24" fill="currentColor" aria-hidden="true" focusable="false">
      <path fillRule="evenodd" d={`${BOWS[hash(id) % BOWS.length]}${BOW_HOLE}`} />
      <path d={bladePath(cutsOf(id))} />
    </svg>
  );
}

/** How much of the rotation interval is used, from 0 to 1, as a row of ticks. */
export function LifeRule({ used, tone = "ok" }: { used: number; tone?: "ok" | "warn" }) {
  const ticks = 16;
  const filled = Math.round(Math.min(1, Math.max(0, used)) * ticks);
  return (
    <svg className={`life life-${tone}`} width={64} height={6} viewBox="0 0 64 6" aria-hidden="true" focusable="false">
      {Array.from({ length: ticks }, (_, index) => (
        <rect key={index} x={index * 4} width={2} height={6} rx={1} className={index < filled ? "life-on" : "life-off"} />
      ))}
    </svg>
  );
}

/** The mark of the app: a key, hung from its bow. */
export function BrandMark({ size = 30 }: { size?: number }) {
  return (
    <svg className="brandmark" width={size} height={size} viewBox="0 0 32 32" fill="currentColor" aria-hidden="true" focusable="false">
      <path fillRule="evenodd" d="M16 2a8.5 8.5 0 1 0 0 17a8.5 8.5 0 1 0 0-17zM16 6.4a2.9 2.9 0 1 1 0 5.8a2.9 2.9 0 1 1 0-5.8z" />
      <path d="M14.2 17.5h3.6V30h-3.6zM17.8 21.4h3.9v2.7h-3.9zM17.8 26h2.6v2.7h-2.6z" />
    </svg>
  );
}

const PIN_LENGTHS = [20, 30, 16, 34, 24, 18, 32, 22, 28, 16, 30, 20];
const PIN_SAG = [26, 18, 30, 14, 22, 28, 16, 24, 20, 30, 18, 26];
const PIN_X = PIN_LENGTHS.map((_, index) => 196 + index * 33);
const SHEAR = 124;

function keyProfile(): string {
  const shaft = 172;
  let path = `M150 196V${shaft}`;
  PIN_X.forEach((x, index) => {
    path += `H${x - 12}L${x - 6} ${SHEAR + PIN_LENGTHS[index]}H${x + 6}L${x + 12} ${shaft}`;
  });
  return `${path}H${PIN_X[11] + 22}L${PIN_X[11] + 34} 184L${PIN_X[11] + 22} 196Z`;
}

function spring(x: number): string {
  let path = `M${x} 50`;
  for (let step = 1; step <= 6; step += 1) {
    path += `L${x + (step % 2 ? 6 : -6)} ${50 + step * 7}`;
  }
  return `${path}L${x} 94`;
}

interface LockProps {
  /** Characters typed, of the 12 that a new password needs. */
  filled: number;
  busy?: boolean;
  /** True where the 12-character rule applies. Only then does a full row of pins mean ready. */
  rule?: boolean;
}

/** A pin-tumbler lock in section. Twelve pins, one per character of the shortest new password. */
export function LockCylinder({ filled, busy = false, rule = false }: LockProps) {
  const count = Math.min(12, Math.max(0, filled));
  const open = rule && count === 12;
  const insertion: CSSProperties = { transform: `translateX(${(count / 12 - 1) * 440}px)` };
  return (
    <svg className={`lock${open ? " lock-open" : ""}${busy ? " lock-busy" : ""}`} viewBox="70 20 560 210" aria-hidden="true" focusable="false">
      <rect className="lock-body" x="150" y="28" width="470" height="196" rx="30" />
      <rect className="lock-plug" x="166" y={SHEAR} width="438" height="84" rx="20" />
      {PIN_X.map((x, index) => {
        const set = index < count;
        const sag = set ? 0 : PIN_SAG[index];
        return (
          <g key={x}>
            <rect className="lock-chamber" x={x - 11} y="42" width="22" height="86" rx="5" />
            <path className="lock-spring" d={spring(x)} style={{ transform: `scaleY(${(44 + sag) / 44})`, transformOrigin: `${x}px 50px` }} />
            <g className="lock-pin" style={{ transform: `translateY(${sag}px)` }}>
              <rect className="lock-driver" x={x - 7} y={SHEAR - 30} width="14" height="30" rx="2.5" />
              <rect className="lock-keypin" x={x - 7} y={SHEAR + 1} width="14" height={PIN_LENGTHS[index] - 4} rx="2.5" />
            </g>
          </g>
        );
      })}
      <g className="lock-key" style={insertion}>
        <path d={keyProfile()} />
        <path fillRule="evenodd" d="M104 148a34 34 0 1 0 0 68a34 34 0 1 0 0-68zM92 169a13 13 0 1 1 0 26a13 13 0 1 1 0-26z" transform="translate(16 2)" />
      </g>
      <line className="lock-shear" x1="160" x2="610" y1={SHEAR} y2={SHEAR} />
      <rect className="lock-sweep" x="160" y={SHEAR - 1.5} width="72" height="3" rx="1.5" />
    </svg>
  );
}

const STROKES = { fill: "none", stroke: "currentColor", strokeWidth: 1.5, strokeLinecap: "round", strokeLinejoin: "round" } as const;

/** An empty board of hooks, with the outline of the first key that will hang there. */
export function EmptyHooks() {
  return (
    <svg className="art" width={240} height={112} viewBox="0 0 240 112" aria-hidden="true" focusable="false">
      <g {...STROKES}>
        <rect x="8" y="8" width="224" height="96" rx="10" />
        {[44, 92, 140, 188].map((x) => <path key={x} d={`M${x} 26v8a5 5 0 0 0 10 0`} />)}
      </g>
      <g className="art-brass" {...STROKES} strokeDasharray="3 4">
        <circle cx="49" cy="53" r="11" />
        <path d="M49 64v30M49 80h7M49 88h5" />
      </g>
    </svg>
  );
}

/** A clean bin with its lid set straight. */
export function EmptyBin() {
  return (
    <svg className="art" width={120} height={112} viewBox="0 0 120 112" aria-hidden="true" focusable="false">
      <g {...STROKES}>
        <path d="M24 30h72M46 30V20h28v10M32 30l6 68a6 6 0 0 0 6 6h32a6 6 0 0 0 6-6l6-68" />
        <path d="M52 48v38M68 48v38" />
      </g>
    </svg>
  );
}

/** A ruled page with nothing written on it. */
export function EmptyPage() {
  return (
    <svg className="art" width={120} height={112} viewBox="0 0 120 112" aria-hidden="true" focusable="false">
      <g {...STROKES}>
        <rect x="26" y="10" width="68" height="92" rx="6" />
        <path d="M40 34h40M40 50h40M40 66h40M40 82h22" strokeDasharray="1 5" />
      </g>
    </svg>
  );
}

/** A key seen through a lens: nothing matches. */
export function NoMatch() {
  return (
    <svg className="art" width={120} height={112} viewBox="0 0 120 112" aria-hidden="true" focusable="false">
      <g {...STROKES}>
        <circle cx="52" cy="50" r="34" />
        <path d="M78 76l26 26" />
      </g>
      <g className="art-brass" {...STROKES}>
        <circle cx="40" cy="50" r="7" />
        <path d="M47 50h24M63 50v7M69 50v5" />
      </g>
    </svg>
  );
}
