/// <reference types="vite/client" />
import { expect, test } from "vitest";

// Fault class D on the screen: a word that promises appears in the text of the page only where
// this table names its evidence (a test, the code that keeps it, or docs/THREAT_MODEL.md). A new
// promise needs its evidence first. A limit ("cannot be proved complete") is not a promise, but it
// is listed too, so that every use of the words is read once.

const PROMISE = /\b(?:always|never|verif(?:y|ied)|complete|secure(?:ly)?|safe(?:ly)?|guarantee[sd]?)\b|\bis cleared\b/i;

const ALLOWED: { file: string; text: string; why: string }[] = [
  { file: "./components/Activity.tsx", text: "cannot prove a complete history", why: "a limit: the journal is not authenticated (docs/THREAT_MODEL.md)" },
  { file: "./components/Activity.tsx", text: "(not verified)", why: "a limit: the declared sequence of a damaged line is not checked" },
  { file: "./components/Activity.tsx", text: "History before the damage cannot be proved complete.", why: "a limit, after damage and after a new journal (FR-14)" },
  { file: "./components/RecycleBin.tsx", text: "AKM never empties this list by itself", why: "Vault::purge_recycled_older_than runs only when called, and only the akm CLI calls it" },
  { file: "./components/UseKeys.tsx", text: "Imports never search other scopes", why: "crates/akm-core/tests/vault.rs :: import_never_updates_a_different_project_or_environment" },
  { file: "./App.tsx", text: "decrypt it again to verify the contents before replacement", why: "Vault::commit parses the written bytes again and compares them before it replaces the file" },
];

const sources = import.meta.glob<string>(["./**/*.{ts,tsx}", "!./**/*.test.{ts,tsx}", "!./testing/**"], {
  query: "?raw", import: "default", eager: true,
});

/** The text that the page can show: string literals, template literals and JSX text, without comments. */
function shownText(source: string): string[] {
  const code = source
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .split("\n")
    .map((line) => line.replace(/(^|[^:"'`\\])\/\/.*$/, "$1"))
    .join("\n");
  const literals = [...code.matchAll(/"(?:[^"\\\n]|\\.)*"|`(?:[^`\\]|\\.)*`/g)].map((match) => match[0].slice(1, -1));
  const jsx = [...code.matchAll(/>([^<>{}]*[A-Za-z][^<>{}]*)</g)].map((match) => match[1].replace(/\s+/g, " ").trim());
  return [...literals, ...jsx];
}

const found = Object.entries(sources).flatMap(([file, source]) =>
  shownText(source).filter((text) => PROMISE.test(text)).map((text) => ({ file, text })));

test("every promise word on the screen has its evidence named", () => {
  const unexplained = found.filter(({ file, text }) => !ALLOWED.some((allowed) => allowed.file === file && text.includes(allowed.text)));
  expect(unexplained).toEqual([]);
});

test("every entry of the evidence table still matches a text on the screen", () => {
  const stale = ALLOWED.filter((allowed) => !found.some(({ file, text }) => allowed.file === file && text.includes(allowed.text)));
  expect(stale).toEqual([]);
});

test("the scan finds the words in the source of a view", () => {
  expect(shownText('const a = "It is never shown."; // always\n<p>Kept safe</p>')).toEqual(["It is never shown.", "Kept safe"]);
  expect(found.length).toBeGreaterThan(0);
});
