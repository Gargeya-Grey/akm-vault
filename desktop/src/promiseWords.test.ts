/// <reference types="vite/client" />
import ts from "typescript";
import { expect, test } from "vitest";

// Fault class D on the screen (U2-R1-06): a sentence of the page that holds a word that promises
// must equal, in full, a sentence that this table approves for that file, with its evidence (a
// test, the code that keeps it, or docs/THREAT_MODEL.md). A sentence is approved, never a part of
// one, so an approved sentence cannot carry a second promise beside it. A limit ("cannot be proved
// complete") is not a promise, but it is listed too, so that every use of the words is read once.
//
// The scan reads the source with the TypeScript parser: strings in either quote, template strings
// (an expression reads as {…}) and the text of JSX elements (a child element or an expression reads
// as {…}). It does not read text that the page builds from parts in separate places at run time,
// text from the vault or from native errors, or the texts of the native windows in src-tauri.

const PROMISE = /\b(?:always|never|verif(?:y|ied)|complete|secure(?:ly)?|safe(?:ly)?|guarantee[sd]?)\b|\bis cleared\b/i;
const HOLE = "{…}";

const ALLOWED: { file: string; sentence: string; why: string }[] = [
  { file: "./components/Activity.tsx", sentence: "Records are not authenticated and cannot prove a complete history.", why: "a limit: the journal is not authenticated (docs/THREAT_MODEL.md)" },
  { file: "./components/Activity.tsx", sentence: ", declared sequence {…} (not verified).", why: "a limit: the declared sequence of a damaged line is not checked" },
  { file: "./components/Activity.tsx", sentence: "History before the damage cannot be proved complete.", why: "a limit, after damage and after a new journal (FR-14)" },
  { file: "./components/RecycleBin.tsx", sentence: "Nothing is deleted for good unless you choose it, and AKM never empties this list by itself.", why: "Vault::purge_recycled_older_than runs only when called, and only the akm CLI calls it" },
  { file: "./components/UseKeys.tsx", sentence: "Imports never search other scopes.", why: "crates/akm-core/tests/vault.rs :: import_never_updates_a_different_project_or_environment" },
  { file: "./App.tsx", sentence: "AKM will retain the original encrypted file as a backup, write KDBX 4.1, and decrypt it again to verify the contents before replacement.", why: "Vault::commit parses the written bytes again and compares them before it replaces the file" },
];

const sources = import.meta.glob<string>(["./**/*.{ts,tsx}", "!./**/*.test.{ts,tsx}", "!./testing/**", "!./**/*.d.ts"], {
  query: "?raw", import: "default", eager: true,
});

/** The texts that a file can show, as the parser reads them. Comments, module names and type literals are not texts. */
function shownTexts(file: string, text: string): string[] {
  const source = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true, file.endsWith(".tsx") ? ts.ScriptKind.TSX : ts.ScriptKind.TS);
  const found: string[] = [];
  const child = (node: ts.JsxChild) => {
    if (ts.isJsxText(node)) return node.text;
    if (ts.isJsxExpression(node) && node.expression === undefined) return "";
    if (ts.isJsxExpression(node) && node.expression && (ts.isStringLiteral(node.expression) || ts.isNoSubstitutionTemplateLiteral(node.expression))) return node.expression.text;
    return ` ${HOLE} `;
  };
  const visit = (node: ts.Node) => {
    if (ts.isImportDeclaration(node) || ts.isExportDeclaration(node) || ts.isLiteralTypeNode(node)) return;
    if (ts.isStringLiteral(node) || ts.isNoSubstitutionTemplateLiteral(node)) found.push(node.text);
    else if (ts.isTemplateExpression(node)) found.push([node.head.text, ...node.templateSpans.map((span) => `${HOLE}${span.literal.text}`)].join(""));
    else if (ts.isJsxElement(node) || ts.isJsxFragment(node)) found.push(node.children.map(child).join(""));
    ts.forEachChild(node, visit);
  };
  visit(source);
  return found;
}

/** The sentences of a text, with its spaces folded as the page draws them. */
function sentences(text: string): string[] {
  return text.replace(/\s+/g, " ").trim().split(/(?<=[.!?])\s+(?=\S)/).filter((sentence) => /[A-Za-z]/.test(sentence));
}

function promises(files: Record<string, string>) {
  return Object.entries(files).flatMap(([file, text]) =>
    shownTexts(file, text).flatMap(sentences).filter((sentence) => PROMISE.test(sentence)).map((sentence) => ({ file, sentence })));
}

function unexplained(files: Record<string, string>) {
  return promises(files).filter(({ file, sentence }) => !ALLOWED.some((allowed) => allowed.file === file && allowed.sentence === sentence));
}

test("every sentence on the screen with a promise word equals a sentence that names its evidence", () => {
  expect(unexplained(sources)).toEqual([]);
});

test("every entry of the evidence table still matches a sentence on the screen", () => {
  const found = promises(sources);
  const stale = ALLOWED.filter((allowed) => !found.some(({ file, sentence }) => allowed.file === file && allowed.sentence === sentence));
  expect(stale).toEqual([]);
});

test("the scan reads strings in either quote, template strings and JSX text, and no comment, module name or type", () => {
  const text = [
    `import x from "./never-shown";`,
    `type Kind = "always";`,
    `// It is always safe.`,
    `const a = "Double: kept safe.";`,
    `const b = 'Single: kept safe.';`,
    "const c = `Template: kept safe.`;",
    "const d = `Template ${a} kept safe.`;",
    `const e = <p>Shown <strong>{a}</strong> kept safe. {"Literal: kept safe."}</p>;`,
  ].join("\n");
  expect(promises({ "./components/Planted.tsx": text }).map(({ sentence }) => sentence)).toEqual([
    "Double: kept safe.",
    "Single: kept safe.",
    "Template: kept safe.",
    "Template {…} kept safe.",
    "Shown {…} kept safe.",
    "Literal: kept safe.",
    "Literal: kept safe.",
  ]);
});

test("the review's case: an approved sentence with a second promise after it fails, and only the second sentence is named", () => {
  const recycleBin = "./components/RecycleBin.tsx";
  const planted = sources[recycleBin].replace(
    "AKM never empties this list by itself.",
    "AKM never empties this list by itself. This app guarantees that your secrets are always safe.",
  );
  expect(planted).not.toBe(sources[recycleBin]);
  expect(unexplained({ ...sources, [recycleBin]: planted })).toEqual([
    { file: recycleBin, sentence: "This app guarantees that your secrets are always safe." },
  ]);
});

test("a promise in a template string or a single-quoted string fails", () => {
  const planted = {
    "./components/PlantedTemplate.tsx": "export const a = `Your keys are always safe.`;\nexport const b = (name: string) => `The keys of ${name} are stored securely.`;",
    "./components/PlantedSingle.ts": "export const c = 'AKM guarantees it.';",
  };
  expect(unexplained({ ...sources, ...planted })).toEqual([
    { file: "./components/PlantedTemplate.tsx", sentence: "Your keys are always safe." },
    { file: "./components/PlantedTemplate.tsx", sentence: "The keys of {…} are stored securely." },
    { file: "./components/PlantedSingle.ts", sentence: "AKM guarantees it." },
  ]);
});

test("an approved sentence counts only in its own file, and a longer sentence that holds it does not count", () => {
  const sentence = "Imports never search other scopes.";
  expect(unexplained({ "./components/Other.tsx": `export const a = "${sentence}";` })).toEqual([{ file: "./components/Other.tsx", sentence }]);
  expect(unexplained({ "./components/UseKeys.tsx": `export const a = "Safe: Imports never search other scopes.";` })).toEqual([
    { file: "./components/UseKeys.tsx", sentence: "Safe: Imports never search other scopes." },
  ]);
});
