use hangman_agent::{format_path, losses_at, Lexicon, PlayError};
use std::env;
use std::io::{self, BufRead, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = env::args().skip(1).collect::<Vec<_>>();
    let cmd = args.first().map(String::as_str).unwrap_or("help");
    let lex_path = "data/words.txt";
    match cmd {
        "help" | "--help" | "-h" => {
            println!(
                "\
hangman-agent verify          play every lexicon word and print the proof
hangman-agent show <word>...  show the guesses for a word
hangman-agent play            you think of a word, the bot names letters"
            );
            ExitCode::SUCCESS
        }
        "verify" => {
            let lex = Lexicon::load(lex_path);
            let reports = lex.verify();
            let mut covered = 0usize;
            let mut worst = 0u32;
            let mut lost6 = 0u64;
            let mut lost8 = 0u64;
            let mut lost10 = 0u64;
            println!(
                "{:<5} {:>8} {:>6} {:>6} {:>5}  {}",
                "len", "words", "bound", "worst", "gap", "hardest"
            );
            for report in &reports {
                let line = lex.play(&report.worst_word).unwrap_or_else(|_| {
                    panic!("hardest word {} missing from lexicon", report.worst_word)
                });
                if line.misses != report.worst {
                    eprintln!(
                        "play/verify mismatch on {} (play {}, tree {})",
                        report.worst_word, line.misses, report.worst
                    );
                    return ExitCode::from(1);
                }
                covered += report.words;
                worst = worst.max(report.worst);
                lost6 += losses_at(report, 6);
                lost8 += losses_at(report, 8);
                lost10 += losses_at(report, 10);
                println!(
                    "{:<5} {:>8} {:>6} {:>6} {:>5}  {}  {}",
                    report.len,
                    report.words,
                    report.lower_bound,
                    report.worst,
                    report.worst as i32 - report.lower_bound as i32,
                    report.worst_word,
                    format_path(&report.path)
                );
            }
            println!();
            println!("words {covered}");
            println!("worst misses {worst}");
            println!("lost if 6 misses allowed {lost6}");
            println!("lost if 8 misses allowed {lost8}");
            println!("lost if 10 misses allowed {lost10}");
            println!("lost if {worst} misses allowed 0");
            if covered != lex.total() {
                eprintln!("covered {covered}, lexicon {}", lex.total());
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        "show" => {
            if args.len() < 2 {
                eprintln!("usage: hangman-agent show <word>...");
                return ExitCode::from(2);
            }
            let lex = Lexicon::load(lex_path);
            let mut failed = false;
            for word in args.drain(1..) {
                match lex.play(&word) {
                    Ok(line) => {
                        println!(
                            "{}  length {}  misses {}  {}",
                            line.word,
                            line.word.len(),
                            line.misses,
                            format_path(&line.guesses)
                        );
                    }
                    Err(PlayError::NotInLexicon) => {
                        eprintln!("{word} is not in the lexicon");
                        failed = true;
                    }
                    Err(_) => {
                        eprintln!("{word} is not a word the agent can play");
                        failed = true;
                    }
                }
            }
            if failed {
                ExitCode::from(1)
            } else {
                ExitCode::SUCCESS
            }
        }
        "play" => {
            let lex = Lexicon::load(lex_path);
            if let Err(err) = interact(&lex) {
                eprintln!("{err}");
                ExitCode::from(1)
            } else {
                ExitCode::SUCCESS
            }
        }
        other => {
            eprintln!("unknown command {other}");
            eprintln!("run hangman-agent help");
            ExitCode::from(2)
        }
    }
}

fn interact(lex: &Lexicon) -> Result<(), String> {
    let stdin = io::stdin();
    let mut lines = stdin.lock().lines();
    println!("{} lexicon words.", lex.total());
    println!("How many letters are in the word?");
    let len: usize = read_line(&mut lines)?
        .trim()
        .parse()
        .map_err(|_| "enter the number of letters".to_string())?;
    let group = lex.words_of_length(len);
    if group.is_empty() {
        return Err(format!("no lexicon words of length {len}"));
    }
    println!(
        "{count} {noun} of length {len}. Type the board after each guess, with _ for a blank.",
        count = group.len(),
        noun = if group.len() == 1 { "word" } else { "words" },
    );
    let mut ids: Vec<usize> = (0..group.len()).collect();
    let mut misses = 0u32;
    let mut board = vec![255u8; len];
    loop {
        if ids.len() <= 1 {
            break;
        }
        let Some(letter) = lex.next_letter(len, &ids) else {
            break;
        };
        println!("Guess: {letter}    misses so far: {misses}    words still possible: {}", ids.len());
        let _ = io::stdout().flush();
        let reply = read_line(&mut lines)?;
        let reply = reply.trim().to_ascii_lowercase();
        if reply.len() != len {
            return Err(format!("the board must be {len} characters"));
        }
        let mut mask = 0u32;
        let mut next_board = board.clone();
        for (i, ch) in reply.chars().enumerate() {
            if ch == '_' {
                continue;
            }
            if !ch.is_ascii_lowercase() {
                return Err("use lowercase letters and underscores".into());
            }
            let code = ch as u8 - b'a';
            if board[i] != 255 && board[i] != code {
                return Err(format!("position {} was already {}", i + 1, (board[i] + b'a') as char));
            }
            next_board[i] = code;
            if ch == letter {
                mask |= 1u32 << i;
            }
        }
        let before = ids.len();
        ids.retain(|&id| {
            lex.mask(len, id, letter) == Some(mask)
                && board_matches(group[id].as_bytes(), &next_board)
        });
        if ids.is_empty() {
            return Err(format!(
                "no lexicon word matches {reply}. The guarantee covers the {} listed words.",
                lex.total()
            ));
        }
        if mask == 0 {
            misses += 1;
        }
        board = next_board;
        if ids.len() < before {
            let left = ids.len();
            println!(
                "{left} {} left",
                if left == 1 { "word" } else { "words" }
            );
        }
    }
    let word = &group[ids[0]];
    println!("The word is {word}. Misses: {misses}.");
    Ok(())
}

fn board_matches(word: &[u8], board: &[u8]) -> bool {
    word.len() == board.len()
        && word.iter().zip(board).all(|(&ch, &cell)| cell == 255 || cell == ch - b'a')
}

fn read_line(lines: &mut impl Iterator<Item = io::Result<String>>) -> Result<String, String> {
    lines
        .next()
        .ok_or_else(|| "input closed".to_string())?
        .map_err(|err| err.to_string())
}
