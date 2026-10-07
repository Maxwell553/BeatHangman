import { readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { WORST_MISSES, buildLexicon, play } from "./agent.js";

const lex = buildLexicon(readFileSync(new URL("../data/words.txt", import.meta.url), "utf8"));
const words = process.argv.slice(2);
const rust = spawnSync(
  "cargo",
  ["run", "--release", "--quiet", "--", "show", ...words],
  { cwd: fileURLToPath(new URL("..", import.meta.url)), encoding: "utf8" },
);
if (rust.status !== 0) {
  console.error(rust.stderr);
  process.exit(rust.status ?? 1);
}
const expected = new Map();
for (const line of rust.stdout.trim().split("\n")) {
  const word = line.split("  ")[0];
  const misses = Number(line.match(/misses (\d+)/)[1]);
  expected.set(word, misses);
}
const witnesses = new Set([
  "o", "yr", "zat", "yell", "vizzy", "popper", "buzzing", "dungiest", "mummeries",
  "bocklogged", "bullishness", "amphicarpium", "antambulacral", "chromatography",
  "subquadrangular", "chondrodystrophy", "ultrastructurally", "anthropomorphology",
  "chlorofluorocarbons", "anthropomorphization", "humuhumunukunukuapuaa",
  "disestablishmentarians", "dihdroxycholecalciferol", "pathologicopsychological",
  "philosophicopsychological", "electroencephalographically",
  "antidisestablishmentarianism", "cyclotrimethylenetrinitramine",
  "dichlorodiphenyltrichloroethane",
]);
let failed = 0;
for (const word of words) {
  const line = play(lex, word);
  const want = expected.get(word);
  const budget = WORST_MISSES[word.length];
  const budgetOk = witnesses.has(word) ? want === budget : line.misses <= budget;
  if (line.error || line.misses !== want || !budgetOk) {
    console.error("mismatch", word, "js", line.misses, "rust", want, "budget", budget, line.error ?? "");
    failed += 1;
  }
}
if (failed) process.exit(1);
console.log(`matched ${words.length} words`);
