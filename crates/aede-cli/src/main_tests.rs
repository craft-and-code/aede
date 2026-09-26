//! Tests for the command line itself, split out of `main.rs`.
//!
//! These read the crate's **own source** rather than running it, which is
//! unusual and deliberate: what they check is agreement between things the
//! compiler has no reason to compare — a sentence printed to a user and the
//! table that decides whether the command in that sentence would be accepted.

use super::*;

#[test]
fn every_store_writer_and_the_backup_hold_the_data_lock() {
    for command in [
        "scan",
        "analyze",
        "roots",
        "check",
        "backup",
        "reset",
        "restore",
        "import",
        "sources",
        "review",
        "rules",
        "relation",
        "fetch",
        "missing",
        "merge",
        "fingerprint",
        "collection",
        "love",
        "rate",
        "note",
        "tag",
        "played",
        "history",
    ] {
        assert!(mutates_store(command), "{command} needs the store lock");
    }
    for command in ["serve", "stats", "doctor", "artists", "help"] {
        assert!(
            !mutates_store(command),
            "{command} should not write the store"
        );
    }
}

#[test]
fn every_command_has_a_dedicated_help_page() {
    for (command, _, _) in COMMANDS {
        let page = help::command_page(command);
        assert!(
            page.usage.starts_with("aede "),
            "{command} has no usable syntax"
        );
        assert!(!page.summary.is_empty(), "{command} has no summary");
    }
}

/// Every `.rs` file of this crate's `src`, as text.
fn sources() -> Vec<(String, String)> {
    fn walk(dir: &std::path::Path, found: &mut Vec<(String, String)>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            match path.is_dir() {
                true => walk(&path, found),
                false if path.extension().is_some_and(|e| e == "rs") => {
                    if let Ok(text) = std::fs::read_to_string(&path) {
                        found.push((path.display().to_string(), text));
                    }
                }
                false => {}
            }
        }
    }
    let mut found = Vec::new();
    walk(
        &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut found,
    );
    assert!(found.len() > 20, "the walk found {} files", found.len());
    found
}

/// Every `aede <command> … --<option>` this program prints at a user, as
/// `(file, command, option)`.
///
/// Read out of the source because that is where the sentences are: they are
/// `format!` arguments, scattered across thirty files, and no run of the
/// program exercises more than a handful of them.
///
/// The scan stops at the end of the line, which is what keeps it honest: an
/// option on the next line of the same sentence is missed rather than
/// attributed to the wrong command, and a miss costs nothing while a wrong
/// attribution would cost a failing test nobody could explain.
fn advice() -> Vec<(String, String, String)> {
    let mut found = Vec::new();
    for (file, text) in sources() {
        for line in text.lines() {
            // The table itself, the dispatcher and the help page all mention
            // command names beside option names without advising anything.
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") || trimmed.starts_with("///") {
                continue;
            }
            let mut rest = line;
            while let Some(at) = rest.find("aede ") {
                rest = &rest[at + "aede ".len()..];
                let command: String = rest
                    .chars()
                    .take_while(|c| c.is_ascii_lowercase() || *c == '-')
                    .collect();
                // `aede --data=…` names an option and no command: the global
                // options are typed before any command and belong to none.
                if command.is_empty() || command.starts_with('-') {
                    continue;
                }
                // The rest of this sentence: up to the next `aede`, so two
                // pieces of advice on one line do not lend each other options.
                let tail = match rest.find("aede ") {
                    Some(next) => &rest[..next],
                    None => rest,
                };
                let mut after = tail;
                while let Some(at) = after.find("--") {
                    after = &after[at + 2..];
                    // Digits count: `--m3u` is not `--m`, and a scan that cut
                    // at the `3` would report an option nobody wrote.
                    let option: String = after
                        .chars()
                        .take_while(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-')
                        .collect();
                    if !option.is_empty() {
                        found.push((file.clone(), command.clone(), option));
                    }
                }
            }
        }
    }
    assert!(
        found.len() > 15,
        "the program advises {} command lines: the scan stopped working",
        found.len()
    );
    found
}

#[test]
fn every_command_line_this_program_advises_would_be_accepted() {
    // **An option nobody can type is a feature nobody has**, and the worst
    // shape of it is a program advising it of itself: `aede artist --members`
    // told a reader with no line-up to run `aede fetch --artists --full`, and
    // `--artists` belongs to `playlist`. The advice was refused by this very
    // binary, in the next breath, by the table two hundred lines above.
    //
    // Nothing else can catch it. The sentence is a `format!` argument in one
    // file and the rule is a table in another, and no test that runs the
    // program will ever type every sentence it can print.
    let mut wrong: Vec<String> = Vec::new();
    for (file, command, option) in advice() {
        let named = canonical(&command);
        if !COMMANDS.iter().any(|(name, _, _)| *name == named) {
            wrong.push(format!("{file}: aede {command} — no such command"));
            continue;
        }
        // An option absent from the global list is not an option at all: the
        // binary refuses it before anything else happens.
        if !OPTIONS.contains(&option.as_str()) {
            wrong.push(format!(
                "{file}: aede {command} --{option} — no such option"
            ));
            continue;
        }
        // And one that is restricted has to be restricted to this command.
        if let Some((_, commands, what)) = OPTION_SCOPE.iter().find(|(o, _, _)| *o == option)
            && !commands.contains(&named)
        {
            wrong.push(format!(
                "{file}: aede {command} --{option} — refused, because \"{command}\" \
                 cannot {what}: --{option} applies to {}",
                commands.join(", ")
            ));
        }
    }
    wrong.sort();
    wrong.dedup();
    assert!(
        wrong.is_empty(),
        "command lines this program advises and would then refuse:\n  {}",
        wrong.join("\n  ")
    );
}

#[test]
fn no_sentence_carries_the_whitespace_of_the_source_it_was_written_in() {
    // A long message is written across several source lines with a trailing
    // `\`, which drops the newline **and** the indentation of the next line.
    // Lose that backslash and nothing fails: it compiles, every test passes,
    // and the user reads a sentence with eighteen spaces in the middle of it.
    // That shipped. This is what would have stopped it.
    //
    // **Deliberately narrow.** It looks for the shape the fault actually takes
    // once a formatter has folded the literal onto one line: a run of spaces
    // inside an open string, in something long enough to be a sentence. A
    // wider rule was tried — flagging any literal still open at the end of its
    // line — and it reported eighteen perfectly good continuations, which is
    // how a test becomes something people switch off.
    let mut ragged: Vec<String> = Vec::new();
    for (file, text) in sources() {
        // Help pages are laid-out blocks where the spacing *is* the layout.
        // This file holds the needle it is looking for — a check whose
        // subject is source text must not read its own example, which is the
        // same rule that stops the manual's link check at `docs/`.
        if file.ends_with("help.rs") || file.ends_with("main.rs") || file.ends_with("main_tests.rs")
        {
            continue;
        }
        for (number, line) in text.lines().enumerate() {
            if line.trim_start().starts_with("//") {
                continue;
            }
            if line.split_whitespace().count() <= 4 {
                continue;
            }
            // **Every** run, not the first. The first is the line's own
            // indentation, which is how the first version of this test passed
            // over the very sentence it was written for.
            //
            // Five spaces rather than two, so that the deliberate ones — a
            // ruler, an indent inside a printed block — are left alone. No
            // sentence needs five.
            let mut at = 0;
            while let Some(found) = line[at..].find("     ") {
                at += found;
                // Inside a string: a run of spaces with an even number of
                // quotes before it is indentation, and indentation is not a
                // sentence.
                if line[..at].matches('"').count() % 2 == 1 {
                    ragged.push(format!("{file}:{}: {}", number + 1, line.trim()));
                    break;
                }
                at += 5;
            }
        }
    }
    assert!(
        ragged.is_empty(),
        "sentences carrying the whitespace of the source they were written in \
         — a lost `\\` at the end of a line:\n  {}",
        ragged.join("\n  ")
    );
}
