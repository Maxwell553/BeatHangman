//! Hangman agent.
//!
//! The secret is some word from a fixed lexicon. Length is visible. A guess
//! names one letter and returns the mask of positions where that letter sits,
//! or a miss. The agent wins when only one lexicon word is still possible.
//!
//! A neural net cannot certify that. The agent searches the remaining words.
//! It chooses the letter that minimizes a proven lower bound on the worst
//! branch, then repeats. `verify` plays that policy against every word.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

/// Position mask: bit i is set when the letter occupies index i.
pub type WordMasks = [u32; 26];

#[derive(Clone, Debug)]
pub struct Guess {
    pub letter: char,
    pub hit: bool,
}

#[derive(Clone, Debug)]
pub struct Line {
    pub word: String,
    pub misses: u32,
    pub guesses: Vec<Guess>,
}

#[derive(Clone, Debug)]
pub struct LengthReport {
    pub len: usize,
    pub words: usize,
    pub lower_bound: u32,
    pub worst: u32,
    pub worst_word: String,
    pub path: Vec<Guess>,
    /// How many words finish with each miss count. Index = misses.
    pub histogram: Vec<u64>,
}

#[derive(Debug)]
pub enum PlayError {
    Empty,
    NotAlphabetic,
    NotInLexicon,
}

pub struct Lexicon {
    words: Vec<Vec<String>>,
    spell: Vec<Vec<Vec<u8>>>,
    masks: Vec<Vec<WordMasks>>,
    total: usize,
}

impl Lexicon {
    pub fn load(path: impl AsRef<Path>) -> Self {
        let file = File::open(path.as_ref()).unwrap_or_else(|err| {
            panic!("open {}: {err}", path.as_ref().display());
        });
        let mut words = Vec::new();
        for line in BufReader::new(file).lines() {
            let word = line.unwrap();
            let word = word.trim();
            if word.is_empty() {
                continue;
            }
            if !word.bytes().all(|b| b.is_ascii_lowercase()) {
                panic!("lexicon entry is not a lowercase word: {word}");
            }
            words.push(word.to_string());
        }
        Self::from_words(words)
    }

    pub fn from_words(mut words: Vec<String>) -> Self {
        words.sort();
        words.dedup();
        let mut by_len: Vec<Vec<String>> = Vec::new();
        for word in words {
            let n = word.len();
            if by_len.len() <= n {
                by_len.resize(n + 1, Vec::new());
            }
            by_len[n].push(word);
        }
        let total = by_len.iter().map(|g| g.len()).sum();
        let spell: Vec<Vec<Vec<u8>>> = by_len
            .iter()
            .map(|group| group.iter().map(|w| spell_of(w)).collect())
            .collect();
        let masks: Vec<Vec<WordMasks>> = spell
            .iter()
            .map(|group| group.iter().map(|w| masks_of(w)).collect())
            .collect();
        Self {
            words: by_len,
            spell,
            masks,
            total,
        }
    }

    pub fn total(&self) -> usize {
        self.total
    }

    pub fn contains(&self, word: &str) -> bool {
        let Some(group) = self.words.get(word.len()) else {
            return false;
        };
        group.binary_search_by(|w| w.as_str().cmp(word)).is_ok()
    }

    /// Wrong guesses used to identify `secret`. The word must be in the lexicon.
    pub fn play(&self, secret: &str) -> Result<Line, PlayError> {
        let secret = secret.trim().to_ascii_lowercase();
        if secret.is_empty() {
            return Err(PlayError::Empty);
        }
        if !secret.bytes().all(|b| b.is_ascii_alphabetic()) {
            return Err(PlayError::NotAlphabetic);
        }
        let secret = secret.to_ascii_lowercase();
        let len = secret.len();
        let group = self.words.get(len).ok_or(PlayError::NotInLexicon)?;
        let place = group
            .binary_search(&secret)
            .map_err(|_| PlayError::NotInLexicon)?;
        let masks = &self.masks[len];
        let spell = &self.spell[len];
        let mut ids: Vec<usize> = (0..group.len()).collect();
        let mut guesses = Vec::new();
        let mut misses = 0u32;
        loop {
            if ids.len() <= 1 {
                break;
            }
            let Some(letter) = choose_letter(&ids, masks, spell) else {
                break;
            };
            let mask = masks[place][letter];
            let hit = mask != 0;
            if !hit {
                misses += 1;
            }
            guesses.push(Guess {
                letter: letter_char(letter),
                hit,
            });
            ids.retain(|&id| masks[id][letter] == mask);
        }
        Ok(Line {
            word: secret,
            misses,
            guesses,
        })
    }

    /// Next letter given the words still possible at this length, as indexes.
    pub fn next_letter(&self, len: usize, ids: &[usize]) -> Option<char> {
        if len >= self.masks.len() || ids.len() <= 1 {
            return None;
        }
        choose_letter(ids, &self.masks[len], &self.spell[len]).map(letter_char)
    }

    pub fn words_of_length(&self, len: usize) -> &[String] {
        self.words.get(len).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn mask(&self, len: usize, index: usize, letter: char) -> Option<u32> {
        let letter = (letter as u8).wrapping_sub(b'a') as usize;
        if letter >= 26 {
            return None;
        }
        self.masks.get(len)?.get(index).map(|m| m[letter])
    }

    /// Play every word. The histogram is the proof: each word's miss count is recorded.
    pub fn verify(&self) -> Vec<LengthReport> {
        let mut reports = Vec::new();
        for len in 0..self.words.len() {
            if self.words[len].is_empty() {
                continue;
            }
            if self.words[len].len() > 100 {
                eprintln!("length {len} ({} words)", self.words[len].len());
            }
            reports.push(self.verify_length(len));
        }
        reports
    }

    fn verify_length(&self, len: usize) -> LengthReport {
        let text = &self.words[len];
        let spell = &self.spell[len];
        let masks = &self.masks[len];
        let mut ids: Vec<usize> = (0..text.len()).collect();
        let lower_bound = lower_bound(&ids, spell);
        let mut stats = WalkStats {
            histogram: Vec::new(),
            worst: 0,
            worst_word: String::new(),
            path: Vec::new(),
            covered: 0,
        };
        let mut path = Vec::new();
        walk(&mut ids, masks, spell, text, 0, &mut path, &mut stats);
        assert_eq!(
            stats.covered, text.len() as u64,
            "length {len} covered {} of {} words",
            stats.covered,
            text.len()
        );
        assert!(
            stats.worst >= lower_bound,
            "strategy beat a lower bound at length {len}"
        );
        LengthReport {
            len,
            words: text.len(),
            lower_bound,
            worst: stats.worst,
            worst_word: stats.worst_word,
            path: stats.path,
            histogram: stats.histogram,
        }
    }
}

struct WalkStats {
    histogram: Vec<u64>,
    worst: u32,
    worst_word: String,
    path: Vec<Guess>,
    covered: u64,
}

fn spell_of(word: &str) -> Vec<u8> {
    word.bytes().map(|b| b - b'a').collect()
}

fn masks_of(word: &[u8]) -> WordMasks {
    let mut masks = [0u32; 26];
    for (i, &ch) in word.iter().enumerate() {
        masks[ch as usize] |= 1u32 << i;
    }
    masks
}

fn letter_char(letter: usize) -> char {
    (b'a' + letter as u8) as char
}

/// Worst-case misses forced by any strategy on this set.
///
/// Words that match everywhere except one index, and whose letters in that
/// index do not occur in the shared letters, are separated only by guessing
/// those missing letters. A wrong guess removes one of them. One word in the
/// family therefore costs `count - 1` misses under every policy.
pub fn lower_bound(ids: &[usize], spell: &[Vec<u8>]) -> u32 {
    let n = ids.len();
    if n <= 1 {
        return 0;
    }
    let len = spell[ids[0]].len();
    let pack = len == 0 || (len - 1) * 5 <= 128;
    let mut best = 0u32;
    let mut groups: HashMap<u128, (u32, u32)> = HashMap::with_capacity(n.min(4096));
    let mut long_groups: HashMap<Vec<u8>, (u32, u32)> = HashMap::new();
    for skip in 0..len {
        groups.clear();
        long_groups.clear();
        for &id in ids {
            let word = &spell[id];
            let mut fixed = 0u32;
            let mut sig = 0u128;
            let mut long_sig = Vec::new();
            for (j, &ch) in word.iter().enumerate() {
                if j == skip {
                    continue;
                }
                fixed |= 1u32 << ch;
                if pack {
                    sig = (sig << 5) | u128::from(ch + 1);
                } else {
                    long_sig.push(ch);
                }
            }
            let ch = u32::from(word[skip]);
            let blanks = if pack {
                &mut groups.entry(sig).or_insert((fixed, 0)).1
            } else {
                &mut long_groups.entry(long_sig).or_insert((fixed, 0)).1
            };
            if fixed & (1u32 << ch) == 0 {
                *blanks |= 1u32 << ch;
            }
        }
        for &(_, blanks) in groups.values() {
            let k = blanks.count_ones();
            if k > 0 {
                best = best.max(k - 1);
            }
        }
        for &(_, blanks) in long_groups.values() {
            let k = blanks.count_ones();
            if k > 0 {
                best = best.max(k - 1);
            }
        }
        if best as usize + 1 >= n {
            break;
        }
    }
    best
}

/// Letter that minimizes the worst lower bound over the masks it would produce.
pub fn choose_letter(ids: &[usize], masks: &[WordMasks], spell: &[Vec<u8>]) -> Option<usize> {
    if ids.len() <= 1 {
        return None;
    }
    // Small sets are solved exactly. The family bound is a lower bound, not
    // always the true cost, and this is where a one-step choice gives it away.
    if ids.len() <= 16 {
        return exact_letter(ids, masks, spell);
    }
    let mut best: Option<(u32, u32, u32, u64, usize)> = None;
    let mut buckets: HashMap<u32, Vec<usize>> = HashMap::with_capacity(32);
    for letter in 0..26 {
        buckets.clear();
        for &id in ids {
            buckets.entry(masks[id][letter]).or_default().push(id);
        }
        if buckets.len() <= 1 {
            continue;
        }
        let mut worst = 0u32;
        let mut max_bucket = 0u32;
        let mut sumsq = 0u64;
        let mut miss = 0u32;
        for (&mask, members) in &buckets {
            let add = u32::from(mask == 0);
            worst = worst.max(add + lower_bound(members, spell));
            let count = members.len() as u32;
            max_bucket = max_bucket.max(count);
            sumsq += u64::from(count) * u64::from(count);
            if mask == 0 {
                miss = count;
            }
        }
        let key = (worst, miss, max_bucket, sumsq, letter);
        if best.map(|current| key < current).unwrap_or(true) {
            best = Some(key);
        }
    }
    best.map(|key| key.4)
}

/// Minimax letter for a set of at most 31 words. Returns the letter whose
/// worst mask is cheapest, measured in misses.
fn exact_letter(ids: &[usize], masks: &[WordMasks], spell: &[Vec<u8>]) -> Option<usize> {
    let n = ids.len();
    if n <= 1 || n > 31 {
        return None;
    }
    let full = (1u32 << n) - 1;
    let mut memo: HashMap<u32, u16> = HashMap::new();
    let packed = search_exact(full, n, ids, masks, spell, &mut memo);
    let letter = (packed >> 8) as usize;
    if letter >= 26 { None } else { Some(letter) }
}

fn search_exact(
    alive: u32,
    n: usize,
    ids: &[usize],
    masks: &[WordMasks],
    spell: &[Vec<u8>],
    memo: &mut HashMap<u32, u16>,
) -> u16 {
    if alive.count_ones() <= 1 {
        return 0;
    }
    if let Some(&cached) = memo.get(&alive) {
        return cached;
    }
    let members: Vec<usize> = (0..n)
        .filter(|&i| alive & (1u32 << i) != 0)
        .map(|i| ids[i])
        .collect();
    let bound = lower_bound(&members, spell);
    let mut ranking: Vec<(u32, u32, usize)> = Vec::new();
    for letter in 0..26 {
        let mut buckets: Vec<(u32, u32)> = Vec::new();
        let mut local = alive;
        while local != 0 {
            let i = local.trailing_zeros() as usize;
            local &= local - 1;
            let mask = masks[ids[i]][letter];
            if let Some(slot) = buckets.iter_mut().find(|slot| slot.0 == mask) {
                slot.1 += 1;
            } else {
                buckets.push((mask, 1));
            }
        }
        if buckets.len() <= 1 {
            continue;
        }
        let miss = buckets.iter().find(|slot| slot.0 == 0).map(|slot| slot.1).unwrap_or(0);
        let max_bucket = buckets.iter().map(|slot| slot.1).max().unwrap_or(0);
        ranking.push((miss, max_bucket, letter));
    }
    ranking.sort_unstable();
    if ranking.is_empty() {
        // Cost 0. Letter 255 means there is nothing to guess.
        let packed = 0xff00;
        memo.insert(alive, packed);
        return packed;
    }
    let mut best_cost = 41u16;
    let mut best_letter = ranking[0].2 as u16;
    for (_, _, letter) in ranking {
        let mut groups: Vec<(u32, u32)> = Vec::new();
        let mut local = alive;
        while local != 0 {
            let i = local.trailing_zeros() as usize;
            local &= local - 1;
            let mask = masks[ids[i]][letter];
            if let Some(slot) = groups.iter_mut().find(|slot| slot.0 == mask) {
                slot.1 |= 1u32 << i;
            } else {
                groups.push((mask, 1u32 << i));
            }
        }
        let mut worst = 0u16;
        let mut too_high = false;
        for (mask, sub) in groups {
            let add = u16::from(mask == 0);
            if add >= best_cost {
                too_high = true;
                break;
            }
            let sub_cost = search_exact(sub, n, ids, masks, spell, memo) & 0xff;
            worst = worst.max(add + sub_cost);
            if worst >= best_cost {
                too_high = true;
                break;
            }
        }
        if !too_high && worst < best_cost {
            best_cost = worst;
            best_letter = letter as u16;
            if best_cost as u32 == bound {
                break;
            }
        }
    }
    let packed = best_cost | (best_letter << 8);
    memo.insert(alive, packed);
    packed
}

fn walk(
    ids: &mut [usize],
    masks: &[WordMasks],
    spell: &[Vec<u8>],
    text: &[String],
    misses: u32,
    path: &mut Vec<Guess>,
    stats: &mut WalkStats,
) {
    let letter = choose_letter(ids, masks, spell);
    let Some(letter) = letter else {
        let bucket = misses as usize;
        if stats.histogram.len() <= bucket {
            stats.histogram.resize(bucket + 1, 0);
        }
        stats.histogram[bucket] += ids.len() as u64;
        stats.covered += ids.len() as u64;
        if misses >= stats.worst {
            if misses > stats.worst {
                stats.worst = misses;
                stats.worst_word.clear();
            }
            for &id in ids.iter() {
                if stats.worst_word.is_empty() || text[id] < stats.worst_word {
                    stats.worst_word = text[id].clone();
                    stats.path = path.clone();
                }
            }
        }
        return;
    };

    let mut order: Vec<(u32, usize)> = ids.iter().copied().map(|id| (masks[id][letter], id)).collect();
    order.sort_unstable_by_key(|item| item.0);
    let mut start = 0;
    while start < order.len() {
        let mask = order[start].0;
        let mut end = start + 1;
        while end < order.len() && order[end].0 == mask {
            end += 1;
        }
        for (slot, item) in order[start..end].iter().enumerate() {
            ids[start + slot] = item.1;
        }
        path.push(Guess {
            letter: letter_char(letter),
            hit: mask != 0,
        });
        walk(
            &mut ids[start..end],
            masks,
            spell,
            text,
            misses + u32::from(mask == 0),
            path,
            stats,
        );
        path.pop();
        start = end;
    }
}

pub fn format_path(path: &[Guess]) -> String {
    path.iter()
        .map(|guess| format!("{}{}", guess.letter, if guess.hit { "+" } else { "-" }))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Words that would be lost if the gallows allowed only `lives` wrong guesses.
pub fn losses_at(report: &LengthReport, lives: u32) -> u64 {
    report
        .histogram
        .iter()
        .enumerate()
        .filter(|(misses, _)| *misses as u32 > lives)
        .map(|(_, n)| *n)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toy(words: &[&str]) -> Lexicon {
        Lexicon::from_words(words.iter().map(|w| (*w).to_string()).collect())
    }

    #[test]
    fn one_letter_family_forces_two_misses_and_the_agent_meets_it() {
        let lex = toy(&["a", "i", "o"]);
        let report = lex.verify();
        assert_eq!(report.len(), 1);
        assert_eq!(report[0].lower_bound, 2);
        assert_eq!(report[0].worst, 2);
        for word in ["a", "i", "o"] {
            assert!(lex.play(word).unwrap().misses <= 2);
        }
    }

    #[test]
    fn known_word_spells_its_letters_with_no_search() {
        let lex = toy(&["jazz"]);
        let line = lex.play("jazz").unwrap();
        assert_eq!(line.misses, 0);
        assert!(line.guesses.is_empty() || line.guesses.iter().all(|g| g.hit));
    }

    #[test]
    fn rejects_a_word_outside_the_list() {
        let lex = toy(&["jazz"]);
        assert!(matches!(lex.play("quiz"), Err(PlayError::NotInLexicon)));
    }
}
