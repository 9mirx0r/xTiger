//! The `explain` subcommand: what a report key means, how to fix it and how to silence it.

use std::str::FromStr;

use anyhow::{Result, bail};
use tiger_lib::{ErrorKey, all_keys, key_info};

const WIDTH: usize = 78;

/// Print the description of `key`, or the list of all keys when there is none.
pub fn explain(key: Option<&str>) -> Result<()> {
    let Some(name) = key else {
        list_keys();
        return Ok(());
    };
    let name = name.trim().trim_start_matches('(').trim_end_matches(')').to_ascii_lowercase();
    let Ok(key) = ErrorKey::from_str(&name) else {
        let near: Vec<_> = all_keys()
            .map(|k| k.to_string())
            .filter(|k| {
                (name.len() >= 3 && (k.contains(&name) || name.contains(k.as_str())))
                    || distance(k, &name) <= 2
            })
            .collect();
        if near.is_empty() {
            bail!("`{name}` is not a report key. Run `explain` without a key to list them all.");
        }
        bail!("`{name}` is not a report key. Did you mean: {}?", near.join(", "));
    };
    let info = key_info(key);
    println!("{key}");
    println!("  {}", info.summary);
    println!();
    println!("What it means");
    print_wrapped(info.meaning);
    println!();
    println!("How to fix it");
    print_wrapped(info.fix);
    println!();
    println!("How to silence it");
    print_wrapped(&format!(
        "One place: put `#tiger-ignore(key={key})` on the line before it. Use \
         `#tiger-ignore(block, key={key})` for a whole block or `#tiger-ignore(file, key={key})` \
         for a whole file (see annotations.md)."
    ));
    print_wrapped(&format!("This run only: add `--skip-key {key}`."));
    print_wrapped(&format!(
        "Every run: in ck3-tiger.conf, inside `filter = {{ trigger = {{ ... }} }}`, add \
         `NOT = {{ key = {key} }}`."
    ));
    println!();
    println!("To see only this key, run with `--only-key {key}`.");
    Ok(())
}

fn list_keys() {
    println!("Report keys. Run `explain <key>` for the details of one.");
    println!();
    let width = all_keys().map(|k| k.to_string().len()).max().unwrap_or(0);
    for key in all_keys() {
        println!("  {:width$}  {}", key.to_string(), key_info(key).summary);
    }
}

/// Print `text` indented by two spaces and wrapped at `WIDTH` columns.
fn print_wrapped(text: &str) {
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.len() + 1 + word.len() > WIDTH - 2 {
            println!("  {line}");
            line.clear();
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        println!("  {line}");
    }
}

/// Number of single-character edits between `a` and `b`.
fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let above = row[j + 1];
            row[j + 1] = (row[j] + 1).min(above + 1).min(diagonal + usize::from(ca != *cb));
            diagonal = above;
        }
    }
    row[b.len()]
}
