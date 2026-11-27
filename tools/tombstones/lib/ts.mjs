// Finds where a tombstone mark goes in TypeScript/TSX: named functions and methods, and (blocks: true) if/else/catch/case blocks.
// Uses the TypeScript compiler's own parser, so strings, regexes, JSX and comments never confuse it.

/**
 * @returns {Array<{bodyOpen:number,start:number,end:number,symbol:string,kind:string,span?:[number,number]}>}
 * `span` (only on whole declarations that can be commented out in one piece) is what `deprecate` comments out.
 */
export function findTs(ts, src, fileName, { blocks = false } = {}) {
  const sf = ts.createSourceFile(fileName, src, ts.ScriptTarget.Latest, true, /\.tsx$/.test(fileName) ? ts.ScriptKind.TSX : ts.ScriptKind.TS);
  const out = [];
  const K = ts.SyntaxKind;
  const textOf = (n) => (n ? n.getText(sf) : "");

  function startsWithDirective(block) {
    const s = block.statements[0];
    return !!s && ts.isExpressionStatement(s) && ts.isStringLiteral(s.expression);
  }
  function nameOfFn(node) {
    if (ts.isFunctionDeclaration(node) || ts.isFunctionExpression(node)) if (node.name) return node.name.text;
    if (ts.isMethodDeclaration(node) || ts.isGetAccessorDeclaration(node) || ts.isSetAccessorDeclaration(node)) {
      const n = textOf(node.name).replace(/^["']|["']$/g, "");
      return ts.isGetAccessorDeclaration(node) ? `get ${n}` : ts.isSetAccessorDeclaration(node) ? `set ${n}` : n;
    }
    const p = node.parent;
    if (p && (ts.isVariableDeclaration(p) || ts.isPropertyAssignment(p) || ts.isPropertyDeclaration(p)) && p.initializer === node) return textOf(p.name);
    return null; // an anonymous callback: its enclosing function carries the mark
  }
  function spanOf(node) {
    if (ts.isFunctionDeclaration(node) && ts.isSourceFile(node.parent)) return [node.getStart(sf), node.end];
    if (ts.isMethodDeclaration(node) && ts.isClassDeclaration(node.parent)) return [node.getStart(sf), node.end];
    const d = node.parent;
    if (d && ts.isVariableDeclaration(d) && d.initializer === node) {
      const list = d.parent, stmt = list?.parent;
      if (list && list.declarations.length === 1 && stmt && ts.isVariableStatement(stmt) && ts.isSourceFile(stmt.parent)) return [stmt.getStart(sf), stmt.end];
    }
    return undefined;
  }
  function block(body, scope, tag, kind, node) {
    if (!blocks || !scope.length) return;
    out.push({ bodyOpen: body.getStart(sf), start: node.getStart(sf), end: body.end, symbol: `${scope.join(".")}#${tag}`, kind });
  }

  function visit(node, scope) {
    let inner = scope;
    const isFn = ts.isFunctionDeclaration(node) || ts.isFunctionExpression(node) || ts.isArrowFunction(node)
      || ts.isMethodDeclaration(node) || ts.isGetAccessorDeclaration(node) || ts.isSetAccessorDeclaration(node);
    if (isFn) {
      const name = nameOfFn(node);
      if (name && node.body && ts.isBlock(node.body) && !startsWithDirective(node.body)) {
        const symbol = [...scope, name].join(".");
        out.push({
          bodyOpen: node.body.getStart(sf), start: node.getStart(sf), end: node.end, symbol,
          kind: ts.isMethodDeclaration(node) || ts.isGetAccessorDeclaration(node) || ts.isSetAccessorDeclaration(node) ? "method" : "function",
          span: spanOf(node),
        });
      }
      if (name) inner = [...scope, name];
    } else if (ts.isClassDeclaration(node) || ts.isClassExpression(node)) {
      const n = node.name ? node.name.text : null;
      if (n) inner = [...scope, n];
    } else if (ts.isIfStatement(node)) {
      if (ts.isBlock(node.thenStatement)) block(node.thenStatement, scope, "if", "if", node);
      if (node.elseStatement && ts.isBlock(node.elseStatement)) block(node.elseStatement, scope, "else", "else", node.elseStatement);
    } else if (ts.isCatchClause(node)) {
      block(node.block, scope, "catch", "catch", node);
    } else if (blocks && scope.length && (node.kind === K.CaseClause || node.kind === K.DefaultClause) && node.statements.length) {
      const label = node.kind === K.CaseClause ? `case:${textOf(node.expression).slice(0, 30)}` : "default";
      // the mark goes just after the `:`; there is no `{` to hang it on, so use a pseudo body open one char before
      out.push({ bodyOpen: node.statements.pos - 1, start: node.getStart(sf), end: node.end, symbol: `${scope.join(".")}#${label}`, kind: "case" });
    }
    ts.forEachChild(node, (c) => visit(c, inner));
  }
  visit(sf, []);
  out.sort((a, b) => a.bodyOpen - b.bodyOpen);
  return out;
}
