// @vitest-environment node
import ts from "typescript";
import { expect, test } from "vitest";

// U2-R1-01, the guard of fault class M. A page shows a key or a journal record only through the
// row facts (rowFacts.ts), which draw every value and carry the short ID.
//
// How the guard finds the places that list or name a key: it reads every page file with the
// TypeScript type checker, so it knows the type of each object that the code reads.
//  1. Each read of a drawn field (below) of a `KeyView` or an `AuditEntry` outside rowFacts.ts is
//     a raw value on its way to the screen. Reads by property, by a string index and by
//     destructuring count. The table ALLOWED names the reads that draw nothing, with a reason and
//     a count per file and field, so a new read in the same file fails too.
//  2. Each read of the title of the row facts (`KeyRowFacts.title`, `BinRowFacts.title`,
//     `JournalKeyFacts.name`) and each `<KeyCut>` must have the short ID beside it: the function
//     that holds it also reads the `id` of row facts, or calls a helper that draws it.
// The guard does not see a value that leaves a key through a spread, `Object.values` or
// `JSON.stringify`; no page file does that today.

// The folder of this file (src), with forward slashes; TypeScript's own `ts.sys` reads the files.
const SRC = decodeURIComponent(new URL(".", import.meta.url).pathname).replace(/^\/(?=[A-Za-z]:)/, "").replace(/\/$/, "");
const ROOT = SRC.slice(0, SRC.lastIndexOf("/"));
const under = (file: string) => `${SRC}/${file}`;

const DRAWN = new Map<string, ReadonlySet<string>>([
  ["KeyView", new Set(["title", "issuer", "project", "environment", "envVar", "resolvedEnvVar", "url", "notes", "scopes", "tags"])],
  ["AuditEntry", new Set(["keyTitle", "project", "environment"])],
]);
const TITLES = new Map([["KeyRowFacts", "title"], ["BinRowFacts", "title"], ["JournalKeyFacts", "name"]]);
/** The helpers and the component that draw the ID of the facts they are given. */
const DRAWS_ID = new Set(["namedKey", "keyFactList", "binFactList", "keyRowDescription", "binRowDescription", "KeyTarget"]);

const ALLOWED: { file: string; reads: Record<string, number>; why: string }[] = [
  {
    file: "components/KeyForm.tsx",
    reads: { "KeyView.title": 1, "KeyView.issuer": 1, "KeyView.project": 1, "KeyView.environment": 1, "KeyView.envVar": 1, "KeyView.url": 1, "KeyView.scopes": 1, "KeyView.tags": 1, "KeyView.notes": 1 },
    why: "the edit form starts from the stored values, so that a save keeps them; its fields draw nothing else",
  },
  {
    file: "components/UseKeys.tsx",
    reads: { "KeyView.project": 3, "KeyView.environment": 2 },
    why: "Use keys selects by the stored project and environment; FilterSelect draws each option with drawnText",
  },
  {
    file: "components/KeysView.tsx",
    reads: { "KeyView.title": 2 },
    why: "the rows sort by the stored title; each row draws its row facts",
  },
  {
    file: "reminders.ts",
    reads: { "KeyView.title": 2 },
    why: "the attention groups sort by the stored title; the summary names each key with namedKey",
  },
];

const norm = (file: string) => file.replace(/\\/g, "/").toLowerCase();
/** The path of a file under src, or null for a file outside it. */
function relative(file: string): string | null {
  const name = file.replace(/\\/g, "/");
  return norm(name).startsWith(`${norm(SRC)}/`) ? name.slice(SRC.length + 1) : null;
}

function pageFiles(): string[] {
  return ts.sys.readDirectory(SRC, [".ts", ".tsx"], ["**/*.test.ts", "**/*.test.tsx", "**/*.d.ts", "testing/**"], ["**/*"]);
}

const config = ts.parseJsonConfigFileContent(ts.readConfigFile(`${ROOT}/tsconfig.json`, ts.sys.readFile).config, ts.sys, ROOT);
let previous: ts.Program | undefined;

/** A program over the page files. `planted` replaces or adds files, by their path under src. */
function program(planted: Record<string, string> = {}) {
  const replaced = new Map(Object.entries(planted).map(([file, text]) => [norm(under(file)), text]));
  const host = ts.createCompilerHost(config.options, true);
  const getSourceFile = host.getSourceFile;
  host.getSourceFile = (name, language, ...rest) => {
    const text = replaced.get(norm(name));
    return text === undefined ? getSourceFile.call(host, name, language, ...rest) : ts.createSourceFile(name, text, language, true);
  };
  const fileExists = host.fileExists;
  host.fileExists = (name) => replaced.has(norm(name)) || fileExists.call(host, name);
  const readFile = host.readFile;
  host.readFile = (name) => replaced.get(norm(name)) ?? readFile.call(host, name);
  const roots = [...new Map([...pageFiles(), ...Object.keys(planted).map(under)].map((file) => [norm(file), file])).values()];
  const built = ts.createProgram(roots, { ...config.options, noEmit: true }, host, previous);
  if (Object.keys(planted).length === 0) previous = built;
  return built;
}

const typeNames = (type: ts.Type): string[] =>
  (type.isUnion() ? type.types : [type]).flatMap((part) => [part.getSymbol()?.getName(), part.aliasSymbol?.getName()]).filter((name): name is string => Boolean(name));

interface Finding { file: string; line: number; what: string }

function scan(built: ts.Program) {
  const checker = built.getTypeChecker();
  const raw: Finding[] = [];
  const unnamed: Finding[] = [];
  for (const source of built.getSourceFiles()) {
    const file = relative(source.fileName);
    if (file === null || source.isDeclarationFile || file === "rowFacts.ts" || /\.test\.tsx?$/.test(file) || file.startsWith("testing/")) continue;
    const at = (node: ts.Node) => source.getLineAndCharacterOfPosition(node.getStart(source)).line + 1;
    const fieldRead = (object: ts.Node, field: string, node: ts.Node) => {
      for (const type of typeNames(checker.getTypeAtLocation(object))) {
        if (DRAWN.get(type)?.has(field)) raw.push({ file, line: at(node), what: `${type}.${field}` });
      }
    };
    // Per function: the reads of a key's title from row facts, and whether the function draws the ID.
    const titles = new Map<ts.Node, ts.Node[]>();
    const drawsId = new Set<ts.Node>();
    const owner = (node: ts.Node): ts.Node => {
      for (let up = node.parent; up; up = up.parent) if (ts.isFunctionLike(up)) return up;
      return source;
    };
    const visit = (node: ts.Node) => {
      if (ts.isPropertyAccessExpression(node)) {
        const field = node.name.text;
        fieldRead(node.expression, field, node);
        for (const type of typeNames(checker.getTypeAtLocation(node.expression))) {
          if (TITLES.get(type) === field) titles.set(owner(node), [...(titles.get(owner(node)) ?? []), node]);
          if (TITLES.has(type) && field === "id") drawsId.add(owner(node));
        }
      } else if (ts.isElementAccessExpression(node) && ts.isStringLiteralLike(node.argumentExpression)) {
        fieldRead(node.expression, node.argumentExpression.text, node);
      } else if (ts.isObjectBindingPattern(node)) {
        for (const element of node.elements) {
          const name = element.propertyName ?? element.name;
          if (ts.isIdentifier(name) || ts.isStringLiteralLike(name)) fieldRead(node, name.text, element);
        }
      } else if (ts.isCallExpression(node) && ts.isIdentifier(node.expression) && DRAWS_ID.has(node.expression.text)) {
        drawsId.add(owner(node));
      } else if ((ts.isJsxOpeningElement(node) || ts.isJsxSelfClosingElement(node)) && ts.isIdentifier(node.tagName)) {
        if (DRAWS_ID.has(node.tagName.text)) drawsId.add(owner(node));
        if (node.tagName.text === "KeyCut") titles.set(owner(node), [...(titles.get(owner(node)) ?? []), node]);
      }
      ts.forEachChild(node, visit);
    };
    visit(source);
    for (const [fn, nodes] of titles) if (!drawsId.has(fn)) for (const node of nodes) unnamed.push({ file, line: at(node), what: node.getText(source).slice(0, 40) });
  }
  return { raw, unnamed };
}

function unexplained(raw: Finding[]) {
  const counts = new Map<string, Map<string, number>>();
  for (const { file, what } of raw) {
    const fileCounts = counts.get(file) ?? new Map<string, number>();
    fileCounts.set(what, (fileCounts.get(what) ?? 0) + 1);
    counts.set(file, fileCounts);
  }
  const wrong: string[] = [];
  for (const [file, fileCounts] of counts) {
    const allowed = ALLOWED.find((entry) => entry.file === file)?.reads ?? {};
    for (const [what, count] of fileCounts) {
      if (allowed[what] !== count) wrong.push(`${file}: ${count} raw ${what} (allowed ${allowed[what] ?? 0}) at lines ${raw.filter((each) => each.file === file && each.what === what).map((each) => each.line).join(", ")}`);
    }
  }
  return wrong;
}

test("no page file draws a raw key or journal value, and every read the table allows is still there", { timeout: 120_000 }, () => {
  const { raw } = scan(program());
  expect(unexplained(raw)).toEqual([]);
  const stale = ALLOWED.flatMap((entry) => Object.entries(entry.reads).filter(([what, count]) => raw.filter((each) => each.file === entry.file && each.what === what).length !== count).map(([what]) => `${entry.file} ${what}`));
  expect(stale).toEqual([]);
});

test("every title from row facts and every key glyph has the short ID beside it", { timeout: 120_000 }, () => {
  expect(scan(program()).unnamed).toEqual([]);
});

const PLANTED_LIST = `import type { KeyView } from "../types";
export function PlantedList({ keys }: { keys: KeyView[] }) {
  return <ul>{keys.map((key) => <li key={key.id}>{key.title} · {key["issuer"]}</li>)}</ul>;
}
export function PlantedDestructure({ item }: { item: KeyView }) {
  const { project, environment: env } = item;
  return <p>{project}{env}</p>;
}
`;

test("a new component that lists keys from raw values fails, by property, string index and destructuring", { timeout: 120_000 }, () => {
  const { raw } = scan(program({ "components/PlantedList.tsx": PLANTED_LIST }));
  expect(unexplained(raw)).toEqual([
    "components/PlantedList.tsx: 1 raw KeyView.title (allowed 0) at lines 3",
    "components/PlantedList.tsx: 1 raw KeyView.issuer (allowed 0) at lines 3",
    "components/PlantedList.tsx: 1 raw KeyView.project (allowed 0) at lines 6",
    "components/PlantedList.tsx: 1 raw KeyView.environment (allowed 0) at lines 6",
  ]);
});

const PLANTED_NO_ID = `import type { KeyView } from "../types";
import { keyRowFacts, type ShortId } from "../rowFacts";
import { KeyCut } from "./glyphs";
export function PlantedRow({ item, shortId }: { item: KeyView; shortId: ShortId }) {
  const facts = keyRowFacts(item, 0, shortId);
  return <p>{facts.title}</p>;
}
export function PlantedGlyph({ item }: { item: KeyView }) {
  return <KeyCut id={item.id} />;
}
`;

test("a component that draws a key's title from row facts, or its glyph, without its short ID fails", { timeout: 120_000 }, () => {
  expect(scan(program({ "components/PlantedNoId.tsx": PLANTED_NO_ID })).unnamed).toEqual([
    { file: "components/PlantedNoId.tsx", line: 6, what: "facts.title" },
    { file: "components/PlantedNoId.tsx", line: 9, what: "<KeyCut id={item.id} />" },
  ]);
});

test("a new raw read in a file that the table names fails, because the count changes", { timeout: 120_000 }, () => {
  const form = ts.sys.readFile(under("components/KeyForm.tsx"))!;
  const planted = form.replace("export function KeyForm(", "export const plantedTitle = (item: KeyView) => item.title;\nexport function KeyForm(");
  expect(planted).not.toBe(form);
  expect(unexplained(scan(program({ "components/KeyForm.tsx": planted })).raw)).toEqual([expect.stringMatching(/^components\/KeyForm\.tsx: 2 raw KeyView\.title \(allowed 1\)/)]);
});
