function hash(text: string): number {
  let value = 0x811c9dc5;
  for (let index = 0; index < text.length; index += 1) {
    value ^= text.charCodeAt(index);
    value = Math.imul(value, 0x01000193);
  }
  return value >>> 0;
}

/** The depth of each cut, from 0 to 4. It comes from the entry's ID and never from its secret. */
export function cutsOf(id: string, count = 5): number[] {
  return Array.from({ length: count }, (_, index) => hash(`${id}:${index}`) % 5);
}

const TEETH = [13, 16, 19, 22, 25];

function keyLines(cuts: number[]): string {
  const teeth = cuts.map((depth, index) => `M${TEETH[index]} 6v${(2 + depth * 1.1).toFixed(1)}`).join("");
  return `M8.6 6H26.5${teeth}`;
}

function LineKey({ className, cuts, width, dashed = false }: { className: string; cuts: number[]; width: number; dashed?: boolean }) {
  return (
    <svg className={className} width={width} height={width / 2} viewBox="0 0 28 14" fill="none" stroke="currentColor" strokeWidth={1.2} strokeLinecap="round" strokeDasharray={dashed ? "1.5 2.2" : undefined} aria-hidden="true" focusable="false">
      <circle cx="5" cy="6" r="3.6" />
      <path d={keyLines(cuts)} />
    </svg>
  );
}

/** A fine line key with its own cut, so two keys look a little different before you read them. */
export function KeyCut({ id }: { id: string }) {
  return <LineKey className="keycut" cuts={cutsOf(id)} width={28} />;
}

/** How much of the rotation interval is used, from 0 to 1, as a row of ticks. */
export function LifeRule({ used, tone = "ok" }: { used: number; tone?: "ok" | "warn" }) {
  const ticks = 16;
  const filled = Math.round(Math.min(1, Math.max(0, used)) * ticks);
  return (
    <svg className={`life life-${tone}`} width={64} height={5} viewBox="0 0 64 5" aria-hidden="true" focusable="false">
      {Array.from({ length: ticks }, (_, index) => (
        <rect key={index} x={index * 4} width={2} height={5} rx={1} className={index < filled ? "life-on" : "life-off"} />
      ))}
    </svg>
  );
}

/** The mark of the app: the line key, with a fixed cut. */
export function BrandMark() {
  return <LineKey className="brandmark" cuts={[3, 1, 4, 2, 3]} width={30} />;
}

const PIN_LENGTHS = [20, 26, 16, 28, 22, 18, 27, 21, 25, 16, 26, 20];
const PIN_SAG = [12, 8, 14, 6, 10, 13, 7, 11, 9, 14, 8, 12];
const SHEAR = 36;

interface LockProps {
  /** Characters typed, of the 12 that a new password needs. */
  filled: number;
  busy?: boolean;
  /** True where the 12-character rule applies. Only then does a full row of pins mean ready. */
  rule?: boolean;
}

/** Twelve fine pins on a line, one per character of the shortest new password. A set pin rests on the line and lights. */
export function LockCylinder({ filled, busy = false, rule = false }: LockProps) {
  const count = Math.min(12, Math.max(0, filled));
  const open = rule && count === 12;
  return (
    <svg className={`lock${open ? " lock-open" : ""}${busy ? " lock-busy" : ""}`} viewBox="0 0 288 56" aria-hidden="true" focusable="false">
      <line className="lock-shear" x1="4" x2="284" y1={SHEAR + 0.5} y2={SHEAR + 0.5} />
      {PIN_LENGTHS.map((length, index) => {
        const set = index < count;
        return (
          <rect
            key={index}
            className={`lock-pin${set ? " lock-pin-set" : ""}`}
            x={12 + index * 24}
            y={SHEAR - length}
            width="2"
            height={length}
            rx="1"
            style={{ transform: `translateY(${set ? 0 : PIN_SAG[index]}px)` }}
          />
        );
      })}
      <rect className="lock-sweep" x="4" y={SHEAR - 1} width="32" height="2" rx="1" />
    </svg>
  );
}

const STROKES = { fill: "none", stroke: "currentColor", strokeWidth: 2.8, strokeLinecap: "round", strokeLinejoin: "round" } as const;

/** The outline of the first key, not yet cut. */
export function EmptyHooks() {
  return <LineKey className="art" cuts={[0, 0, 0, 0, 0]} width={64} dashed />;
}

/** A clean bin with its lid set straight. */
export function EmptyBin() {
  return (
    <svg className="art" width={60} height={56} viewBox="0 0 120 112" aria-hidden="true" focusable="false">
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
    <svg className="art" width={60} height={56} viewBox="0 0 120 112" aria-hidden="true" focusable="false">
      <g {...STROKES}>
        <rect x="26" y="10" width="68" height="92" rx="6" />
        <path d="M40 34h40M40 50h40M40 66h40M40 82h22" strokeDasharray="1 5" />
      </g>
    </svg>
  );
}

/** A lens with nothing under it: nothing matches. */
export function NoMatch() {
  return (
    <svg className="art" width={60} height={56} viewBox="0 0 120 112" aria-hidden="true" focusable="false">
      <g {...STROKES}>
        <circle cx="52" cy="50" r="34" />
        <path d="M78 76l26 26" />
      </g>
    </svg>
  );
}
