// Node's existing strip-types runner cannot parse TSX. Transpile UI modules with
// the already-installed TypeScript compiler; no DOM/test framework dependency.
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import ts from "typescript";

export async function resolve(specifier, context, nextResolve) {
  try { return await nextResolve(specifier, context); }
  catch (error) {
    if (!specifier.startsWith(".")) throw error;
    for (const suffix of [".ts", ".tsx", "/index.ts", "/index.tsx"]) {
      try { return await nextResolve(specifier + suffix, context); } catch { /* try next */ }
    }
    throw error;
  }
}
export async function load(url, context, nextLoad) {
  if (!url.includes("/src/") || !/\.tsx?$/.test(url)) return nextLoad(url, context);
  const source = await readFile(fileURLToPath(url), "utf8");
  return {
    format: "module", shortCircuit: true,
    source: ts.transpileModule(source, {
      compilerOptions: { jsx: ts.JsxEmit.ReactJSX, module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2021 },
      fileName: fileURLToPath(url),
    }).outputText,
  };
}
