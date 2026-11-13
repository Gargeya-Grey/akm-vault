import { expect, test } from "vitest";
import { uniqueOptions } from "./components/FilterSelect";
import { drawnLines, drawnText, shortIds } from "./rowFacts";

// U2-R1-01: how a stored value draws, and the short ID of a key.

const CASES: [string, string, string][] = [
  ["a plain value", "Acme Cloud", "Acme Cloud"],
  ["a repeated space", "Acme  Cloud", "Acme ⟨U+0020⟩Cloud"],
  ["three spaces", "a   b", "a ⟨U+0020⟩⟨U+0020⟩b"],
  ["a leading space", " Acme", "⟨U+0020⟩Acme"],
  ["a trailing space", "Acme ", "Acme⟨U+0020⟩"],
  ["a value that is one space", " ", "⟨U+0020⟩"],
  ["a newline", "Line\nbreak", "Line⟨U+000A⟩break"],
  ["a carriage return and a tab", "a\r\tb", "a⟨U+000D⟩⟨U+0009⟩b"],
  ["a zero-width space", "Twin\u200B", "Twin⟨U+200B⟩"],
  ["a zero-width joiner", "a\u200Db", "a⟨U+200D⟩b"],
  ["a byte order mark", "\uFEFFAcme", "⟨U+FEFF⟩Acme"],
  ["a right-to-left override", "\u202Edemo", "⟨U+202E⟩demo"],
  ["a left-to-right mark", "demo\u200E", "demo⟨U+200E⟩"],
  ["a directional isolate", "\u2068demo\u2069", "⟨U+2068⟩demo⟨U+2069⟩"],
  ["a soft hyphen", "co\u00ADop", "co⟨U+00AD⟩op"],
  ["a no-break space", "Acme\u00A0Cloud", "Acme⟨U+00A0⟩Cloud"],
  ["a line separator", "a\u2028b", "a⟨U+2028⟩b"],
  ["a Hangul filler", "a\u3164b", "a⟨U+3164⟩b"],
  ["a variation selector", "a\uFE0Fb", "a⟨U+FE0F⟩b"],
  ["a mark of the drawing itself", "⟨U+0020⟩", "⟨U+27E8⟩U+0020⟨U+27E9⟩"],
  ["the row separator", "Acme · 1 scope", "Acme ⟨U+00B7⟩ 1 scope"],
  ["a character outside the basic plane", "key 🔑", "key 🔑"],
  ["a letter from another script", "\u0410cme", "\u0410cme"],
];

for (const [name, value, drawn] of CASES) {
  test(`${name} draws as ${JSON.stringify(drawn)}`, () => {
    expect(drawnText(value)).toBe(drawn);
  });
}

test("notes keep their line ends as lines, and each line draws by the same rule", () => {
  expect(drawnLines("first line\r\nsecond\u200Bline\n")).toEqual(["first line", "second⟨U+200B⟩line", ""]);
});

test("an option draws its stored value by the rule, and keeps the stored value as its value", () => {
  expect(uniqueOptions(["dev", "dev ", null, "dev", "\u202Eved"])).toEqual([
    { value: "dev", label: "dev" },
    { value: "dev ", label: "dev⟨U+0020⟩" },
    { value: "\u202Eved", label: "⟨U+202E⟩ved" },
  ]);
});

test("two different values never draw the same text", () => {
  const values = [...new Set(CASES.flatMap(([, value, drawn]) => [value, drawn]).concat(["", "a", "a ", " a", "a  ", "a\u200B", "a\u200C", "\u200Ba"]))];
  const drawn = values.map(drawnText);
  expect(new Set(drawn).size).toBe(values.length);
});

test("a drawn value holds no character that draws nothing, moves text or breaks a line", () => {
  const all = Array.from({ length: 0x3000 }, (_, code) => String.fromCodePoint(code)).join("");
  const drawn = drawnText(all);
  expect(drawn).not.toMatch(/[\p{C}\p{Default_Ignorable_Code_Point}]/u);
  expect(drawn).not.toMatch(/(?! )\p{Z}/u);
  expect(drawn).not.toMatch(/ {2}|^ | $/);
});

test("a short ID is the shortest prefix of 8 or more characters that no other ID starts with", () => {
  const ids = shortIds([
    "0fa1c0de-0000-4000-8000-000000000001",
    "0fa1c0de-0001-4000-8000-000000000019",
    "c4d2e8a1-0000-4000-8000-000000000087",
    "c4d2e8a1-1000-4000-8000-000000000002",
    "7b3e9f20-0000-4000-8000-000000000085",
    "short",
    "shortest-id",
  ]);
  expect(Object.fromEntries(ids)).toEqual({
    "0fa1c0de-0000-4000-8000-000000000001": "0fa1c0de-0000",
    "0fa1c0de-0001-4000-8000-000000000019": "0fa1c0de-0001",
    "c4d2e8a1-0000-4000-8000-000000000087": "c4d2e8a1-0",
    "c4d2e8a1-1000-4000-8000-000000000002": "c4d2e8a1-1",
    "7b3e9f20-0000-4000-8000-000000000085": "7b3e9f20",
    short: "short",
    "shortest-id": "shortest",
  });
});

test("an ID that is the start of another ID draws in full, and the longer one draws one character more", () => {
  const ids = shortIds(["abcdefgh", "abcdefghij"]);
  expect(ids.get("abcdefgh")).toBe("abcdefgh");
  expect(ids.get("abcdefghij")).toBe("abcdefghi");
});

test("no two IDs of a set get the same short ID, and each short ID starts its own ID", () => {
  const ids = Array.from({ length: 400 }, (_, index) => `${(index * 2654435761 >>> 0).toString(16).padStart(8, "0").slice(0, 6)}${index % 7}-${index}`);
  const short = shortIds(ids);
  expect(new Set(short.values()).size).toBe(ids.length);
  for (const id of ids) {
    const prefix = short.get(id)!;
    expect(id.startsWith(prefix)).toBe(true);
    expect(prefix.length).toBeGreaterThanOrEqual(Math.min(8, id.length));
    expect(ids.filter((other) => other !== id && other.startsWith(prefix))).toEqual([]);
    if (prefix.length > 8) expect(ids.some((other) => other !== id && other.startsWith(prefix.slice(0, -1)))).toBe(true);
  }
});
