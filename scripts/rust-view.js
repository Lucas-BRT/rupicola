"use strict";

// Projects a Rust source file onto the part the detector can read: the markup
// and class strings inside Topcoat's `view!`, `attributes!` and `class!`
// macros. Everything else becomes spaces. Offsets and newlines are kept, so
// line numbers in findings point at the original file.
//
// Limits: raw strings (r#"..."#) are not understood; a `view!` produced by
// another macro is not seen.

const MACROS = /\b(?:view|attributes|class)!\s*(?=[({\[])/g;
const PAIRS = { "(": ")", "{": "}", "[": "]" };

// Index just past the string literal that opens at `i`.
function skipString(text, i) {
  for (let j = i + 1; j < text.length; j++) {
    if (text[j] === "\\") j++;
    else if (text[j] === '"') return j + 1;
  }
  return text.length;
}

// Index just past a char literal at `i`, or i + 1 for a lifetime.
function skipChar(text, i) {
  const m = /^'(?:\\.[^']*|[^\\'])'/.exec(text.slice(i, i + 12));
  return m ? i + m[0].length : i + 1;
}

// Replaces comments with spaces; strings are left alone.
function blankRustComments(text) {
  const out = text.split("");
  const blank = (from, to) => {
    for (let k = from; k < to; k++) if (out[k] !== "\n") out[k] = " ";
  };
  let i = 0;
  while (i < text.length) {
    const c = text[i];
    if (c === '"') i = skipString(text, i);
    else if (c === "'") i = skipChar(text, i);
    else if (c === "/" && text[i + 1] === "/") {
      const end = text.indexOf("\n", i);
      const stop = end === -1 ? text.length : end;
      blank(i, stop);
      i = stop;
    } else if (c === "/" && text[i + 1] === "*") {
      let depth = 1;
      let k = i + 2;
      while (k < text.length && depth > 0) {
        if (text[k] === "/" && text[k + 1] === "*") { depth++; k += 2; }
        else if (text[k] === "*" && text[k + 1] === "/") { depth--; k += 2; }
        else k++;
      }
      blank(i, k);
      i = k;
    } else i++;
  }
  return out.join("");
}

// Index just past the bracket that closes the one at `open`, skipping strings.
function closeOf(text, open) {
  const opener = text[open];
  const closer = PAIRS[opener];
  let depth = 0;
  for (let i = open; i < text.length; ) {
    const c = text[i];
    if (c === '"') { i = skipString(text, i); continue; }
    if (c === "'") { i = skipChar(text, i); continue; }
    if (c === opener) depth++;
    else if (c === closer && --depth === 0) return i + 1;
    i++;
  }
  return text.length;
}

function project(text) {
  const code = blankRustComments(text);
  const out = code.replace(/[^\n]/g, " ").split("");
  let m;
  MACROS.lastIndex = 0;
  while ((m = MACROS.exec(code))) {
    const open = m.index + m[0].length;
    const end = closeOf(code, open);
    for (let i = m.index; i < end; i++) out[i] = code[i];
    MACROS.lastIndex = end;
  }
  return out.join("");
}

module.exports = { project, blankRustComments };
