// Same policy as src/lib.rs. A letter is chosen by minimizing the worst
// family-bound of the masks it produces. Sets of 16 or fewer words are
// solved by minimax instead of the one-step bound.

export const WORST_MISSES = {
  1: 2, 2: 23, 3: 21, 4: 17, 5: 16, 6: 14, 7: 12, 8: 9, 9: 8, 10: 6,
  11: 6, 12: 4, 13: 4, 14: 3, 15: 3, 16: 3, 17: 3, 18: 2, 19: 2, 20: 1,
  21: 2, 22: 1, 23: 1, 24: 1, 25: 1, 27: 0, 28: 0, 29: 0, 31: 0,
};

const EXACT_LIMIT = 16;

export function buildLexicon(text) {
  const byLen = [];
  for (const raw of text.split("\n")) {
    const word = raw.trim();
    if (!word) continue;
    const len = word.length;
    while (byLen.length <= len) byLen.push([]);
    const spell = new Uint8Array(len);
    const masks = new Uint32Array(26);
    for (let i = 0; i < len; i++) {
      const ch = word.charCodeAt(i) - 97;
      spell[i] = ch;
      masks[ch] |= 1 << i;
    }
    byLen[len].push({ word, spell, masks });
  }
  for (const group of byLen) {
    if (group) group.sort((a, b) => (a.word < b.word ? -1 : a.word > b.word ? 1 : 0));
  }
  return { byLen, total: byLen.reduce((n, g) => n + (g ? g.length : 0), 0) };
}

export function play(lex, secret) {
  const word = secret.trim().toLowerCase();
  if (!/^[a-z]+$/.test(word)) return { error: "not a word" };
  const group = lex.byLen[word.length] || [];
  let lo = 0;
  let hi = group.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (group[mid].word < word) lo = mid + 1;
    else hi = mid;
  }
  if (!group[lo] || group[lo].word !== word) {
    return { error: "not in the lexicon" };
  }
  const place = lo;
  let ids = group.map((_, i) => i);
  const startAlive = ids.length;
  const guesses = [];
  let misses = 0;
  while (ids.length > 1) {
    const letter = chooseLetter(ids, group);
    if (letter === null) break;
    const mask = group[place].masks[letter];
    const hit = mask !== 0;
    if (!hit) misses += 1;
    ids = ids.filter((id) => group[id].masks[letter] === mask);
    guesses.push({ letter: String.fromCharCode(97 + letter), hit, alive: ids.length });
  }
  return { word, misses, guesses, startAlive };
}

function popcount(n) {
  n >>>= 0;
  let count = 0;
  while (n) {
    n &= n - 1;
    count += 1;
  }
  return count;
}

function lowerBound(ids, group) {
  const n = ids.length;
  if (n <= 1) return 0;
  const len = group[ids[0]].spell.length;
  const pack = len === 0 || (len - 1) * 5 <= 128;
  let best = 0;
  for (let skip = 0; skip < len; skip++) {
    const groups = new Map();
    const longGroups = new Map();
    for (const id of ids) {
      const spell = group[id].spell;
      let fixed = 0;
      let sig = 0n;
      let longSig = "";
      for (let j = 0; j < len; j++) {
        if (j === skip) continue;
        const ch = spell[j];
        fixed |= 1 << ch;
        if (pack) sig = (sig << 5n) | BigInt(ch + 1);
        else longSig += String.fromCharCode(ch + 1);
      }
      const ch = spell[skip];
      const table = pack ? groups : longGroups;
      const key = pack ? sig : longSig;
      let entry = table.get(key);
      if (!entry) {
        entry = [fixed, 0];
        table.set(key, entry);
      }
      if ((fixed & (1 << ch)) === 0) entry[1] |= 1 << ch;
    }
    const scan = (blanks) => {
      const k = popcount(blanks);
      if (k > 0) best = Math.max(best, k - 1);
    };
    for (const [, entry] of groups) scan(entry[1]);
    for (const [, entry] of longGroups) scan(entry[1]);
    if (best + 1 >= n) break;
  }
  return best;
}

function chooseLetter(ids, group) {
  if (ids.length <= 1) return null;
  if (ids.length <= EXACT_LIMIT) return exactLetter(ids, group);
  let best = null;
  for (let letter = 0; letter < 26; letter++) {
    const buckets = new Map();
    for (const id of ids) {
      const mask = group[id].masks[letter];
      const list = buckets.get(mask);
      if (list) list.push(id);
      else buckets.set(mask, [id]);
    }
    if (buckets.size <= 1) continue;
    let worst = 0;
    let maxBucket = 0;
    let sumsq = 0;
    let miss = 0;
    for (const [mask, members] of buckets) {
      const add = mask === 0 ? 1 : 0;
      worst = Math.max(worst, add + lowerBound(members, group));
      maxBucket = Math.max(maxBucket, members.length);
      sumsq += members.length * members.length;
      if (mask === 0) miss = members.length;
    }
    const key = [worst, miss, maxBucket, sumsq, letter];
    if (!best || compareKey(key, best.key) < 0) best = { key, letter };
  }
  return best ? best.letter : null;
}

function compareKey(a, b) {
  for (let i = 0; i < a.length; i++) {
    if (a[i] < b[i]) return -1;
    if (a[i] > b[i]) return 1;
  }
  return 0;
}

function exactLetter(ids, group) {
  const n = ids.length;
  if (n <= 1 || n > 31) return null;
  const full = (1 << n) - 1;
  const memo = new Map();
  const packed = searchExact(full, n, ids, group, memo);
  const letter = packed >>> 8;
  return letter >= 26 ? null : letter;
}

function searchExact(alive, n, ids, group, memo) {
  if (popcount(alive) <= 1) return 0;
  const cached = memo.get(alive);
  if (cached !== undefined) return cached;
  const members = [];
  for (let i = 0; i < n; i++) if (alive & (1 << i)) members.push(ids[i]);
  const bound = lowerBound(members, group);
  const ranking = [];
  for (let letter = 0; letter < 26; letter++) {
    const buckets = [];
    for (let local = alive; local; local &= local - 1) {
      const i = Math.clz32(local & -local) ^ 31;
      const mask = group[ids[i]].masks[letter];
      const slot = buckets.find((row) => row[0] === mask);
      if (slot) slot[1] += 1;
      else buckets.push([mask, 1]);
    }
    if (buckets.length <= 1) continue;
    const missRow = buckets.find((row) => row[0] === 0);
    const miss = missRow ? missRow[1] : 0;
    const maxBucket = buckets.reduce((m, row) => Math.max(m, row[1]), 0);
    ranking.push([miss, maxBucket, letter]);
  }
  ranking.sort((a, b) => a[0] - b[0] || a[1] - b[1] || a[2] - b[2]);
  if (ranking.length === 0) {
    memo.set(alive, 0xff00);
    return 0xff00;
  }
  let bestCost = 41;
  let bestLetter = ranking[0][2];
  for (const [, , letter] of ranking) {
    const groups = [];
    for (let local = alive; local; local &= local - 1) {
      const i = Math.clz32(local & -local) ^ 31;
      const mask = group[ids[i]].masks[letter];
      const slot = groups.find((row) => row[0] === mask);
      if (slot) slot[1] |= 1 << i;
      else groups.push([mask, 1 << i]);
    }
    let worst = 0;
    let tooHigh = false;
    for (const [mask, sub] of groups) {
      const add = mask === 0 ? 1 : 0;
      if (add >= bestCost) {
        tooHigh = true;
        break;
      }
      const subCost = searchExact(sub, n, ids, group, memo) & 0xff;
      worst = Math.max(worst, add + subCost);
      if (worst >= bestCost) {
        tooHigh = true;
        break;
      }
    }
    if (!tooHigh && worst < bestCost) {
      bestCost = worst;
      bestLetter = letter;
      if (bestCost === bound) break;
    }
  }
  const packed = bestCost | (bestLetter << 8);
  memo.set(alive, packed);
  return packed;
}
