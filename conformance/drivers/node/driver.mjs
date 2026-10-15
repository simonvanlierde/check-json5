// Conformance driver for the Node parsers. Usage: node driver.mjs <parser> < paths
// Prints "ready", then "<path>\t<accept|reject|crash>" per path. See conformance/README.md.
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { createInterface } from "node:readline";
import JSON5 from "json5";
import stripJsonComments from "strip-json-comments";

// jsonc-parser ships a UMD build, which ESM imports cannot see named exports in.
const { parse: parseJsonc } = createRequire(import.meta.url)("jsonc-parser");

class Rejected extends Error {}

const parsers = {
  "js-json-parse": (text) => JSON.parse(text),
  "js-json5": (text) => JSON5.parse(text),
  "js-strip-json-comments": (text) => JSON.parse(stripJsonComments(text)),
  // parse() never throws: it recovers and collects errors, so any error counts as a reject.
  "js-jsonc-parser": (text) => {
    const errors = [];
    parseJsonc(text, errors);
    if (errors.length > 0) throw new Rejected();
  },
};

const parse = parsers[process.argv[2]];
if (!parse) {
  console.error(`unknown parser: ${process.argv[2]}`);
  process.exit(2);
}
// ignoreBOM keeps a leading BOM in the text, as readFileSync(path, "utf8") does.
const decoder = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true });

function verdict(path) {
  let text;
  try {
    const bytes = readFileSync(path);
    text = decoder.decode(bytes);
  } catch (error) {
    // an unreadable file is a crash, as in the other drivers; invalid UTF-8 never reaches the parser
    return error instanceof TypeError ? "reject" : "crash";
  }
  try {
    parse(text);
    return "accept";
  } catch (error) {
    return error instanceof SyntaxError || error instanceof Rejected ? "reject" : "crash";
  }
}

process.stdout.write("ready\n");
for await (const path of createInterface({ input: process.stdin })) {
  if (path) process.stdout.write(`${path}\t${verdict(path)}\n`);
}
