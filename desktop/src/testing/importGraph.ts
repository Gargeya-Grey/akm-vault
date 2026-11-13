import ts from "typescript";

/**
 * The import graph of the page sources, read with the TypeScript parser (U2-R1-05). A guard asks
 * which files reach a module, by the resolved module and never by the name of a binding, so an
 * alias, a re-export, a dynamic import or a `require` all count. A type-only import does not:
 * it gives no value at run time.
 */

export type Sources = Record<string, string>;

export interface Reach {
  /** The file that reaches the module, as a key of the sources (for example `./components/KeysView.tsx`). */
  file: string;
  /** The resolved module: a key of the sources, or the package name. */
  module: string;
  how: "import" | "re-export" | "dynamic import" | "require" | "import glob";
}

const EXTENSIONS = ["", ".ts", ".tsx", "/index.ts", "/index.tsx"];

/** `./a/b.ts` and `../c` give `./a/../c`, folded to `./c`. A path that leaves the root keeps its `..`. */
function join(from: string, specifier: string): string {
  const parts = [...from.split("/").slice(0, -1), ...specifier.split("/")];
  const folded: string[] = [];
  for (const part of parts) {
    if (part === "." || part === "") continue;
    if (part === ".." && folded.length > 0 && folded[folded.length - 1] !== "..") folded.pop();
    else folded.push(part);
  }
  return folded[0] === ".." ? folded.join("/") : `./${folded.join("/")}`;
}

/** Resolves a specifier from a file to a key of the sources. A package keeps its own name. */
export function resolve(from: string, specifier: string, sources: Sources): string {
  // A Vite query (`?raw`, `?url`) still names the same module.
  const path = specifier.split("?")[0];
  if (!path.startsWith(".")) return path;
  // TypeScript resolves the spelling `./x.js` to `x.ts` or `x.tsx` (U2-R2-03).
  const relative = join(from, path).replace(/\.[cm]?jsx?$/, "");
  for (const extension of EXTENSIONS) if (Object.hasOwn(sources, relative + extension)) return relative + extension;
  return relative;
}

function typeOnlyImport(node: ts.ImportDeclaration): boolean {
  const clause = node.importClause;
  if (!clause) return false; // `import "./x"` runs the module.
  if (clause.isTypeOnly) return true;
  if (clause.name) return false;
  const bindings = clause.namedBindings;
  if (!bindings || ts.isNamespaceImport(bindings)) return false;
  return bindings.elements.length > 0 && bindings.elements.every((element) => element.isTypeOnly);
}

function typeOnlyExport(node: ts.ExportDeclaration): boolean {
  if (node.isTypeOnly) return true;
  const clause = node.exportClause;
  return clause !== undefined && ts.isNamedExports(clause) && clause.elements.length > 0 && clause.elements.every((element) => element.isTypeOnly);
}

function literal(node: ts.Node | undefined): string | null {
  return node !== undefined && (ts.isStringLiteral(node) || ts.isNoSubstitutionTemplateLiteral(node)) ? node.text : null;
}

const parsed = new Map<string, ts.SourceFile>();

/** The syntax tree of a file, parsed once per text. */
export function parse(file: string, text: string): ts.SourceFile {
  const key = `${file}\u0000${text}`;
  let source = parsed.get(key);
  if (source === undefined) {
    source = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true, file.endsWith(".tsx") ? ts.ScriptKind.TSX : ts.ScriptKind.TS);
    parsed.set(key, source);
  }
  return source;
}

/** Every way in which `file` reaches another module at run time. */
export function reachesOf(file: string, sources: Sources): Reach[] {
  const source = parse(file, sources[file]);
  const found: Reach[] = [];
  const add = (specifier: string | null, how: Reach["how"]) => {
    // A specifier that the parser cannot read (a computed one) reaches an unknown module, so it counts as every module.
    found.push({ file, module: specifier === null ? "*" : resolve(file, specifier, sources), how });
  };
  const visit = (node: ts.Node) => {
    if (ts.isImportDeclaration(node) && !typeOnlyImport(node)) add(literal(node.moduleSpecifier), "import");
    else if (ts.isExportDeclaration(node) && node.moduleSpecifier && !typeOnlyExport(node)) add(literal(node.moduleSpecifier), "re-export");
    else if (ts.isImportEqualsDeclaration(node) && !node.isTypeOnly && ts.isExternalModuleReference(node.moduleReference)) add(literal(node.moduleReference.expression), "require");
    else if (ts.isCallExpression(node)) {
      if (node.expression.kind === ts.SyntaxKind.ImportKeyword) add(literal(node.arguments[0]), "dynamic import");
      else if (ts.isIdentifier(node.expression) && node.expression.text === "require") add(literal(node.arguments[0]), "require");
      else if (ts.isPropertyAccessExpression(node.expression) && node.expression.name.text === "glob" && ts.isMetaProperty(node.expression.expression)) add(null, "import glob");
    }
    ts.forEachChild(node, visit);
  };
  visit(source);
  return found;
}

/** Every name in a file that starts with `__TAURI`: an identifier, a property or a string. */
export function tauriGlobalsIn(file: string, sources: Sources): string[] {
  const source = parse(file, sources[file]);
  const found: string[] = [];
  const visit = (node: ts.Node) => {
    if ((ts.isIdentifier(node) || ts.isPrivateIdentifier(node)) && node.text.includes("__TAURI")) found.push(node.text);
    else if ((ts.isStringLiteral(node) || ts.isNoSubstitutionTemplateLiteral(node)) && node.text.includes("__TAURI")) found.push(node.text);
    ts.forEachChild(node, visit);
  };
  visit(source);
  return found;
}

/**
 * The files that reach `module`, with how. A computed specifier or an import glob reaches every
 * module, so it counts here too.
 */
export function importersOf(module: string, sources: Sources): Reach[] {
  return Object.keys(sources).flatMap((file) => reachesOf(file, sources)).filter((reach) => reach.module === module || reach.module === "*");
}

/** The page sources that a guard reads: every module that ships, and no test or test helper. */
export function pageSources(all: Sources): Sources {
  return Object.fromEntries(Object.entries(all).filter(([file]) => !/\.test\.tsx?$/.test(file) && !file.startsWith("./testing/") && !file.endsWith(".d.ts")));
}
