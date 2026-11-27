//! A small SQL shell: type a statement ending in `;` to run it.
//!
//! `\dt` lists the tables, `\di` the indexes, `explain <query>;` prints the plan and `\q` quits. What a client is shown goes to
//! standard output; standard error is the server's log. With no terminal on standard input (a script piped in) the shell prints no
//! greeting and no prompts, only the replies.

use std::io::{self, BufRead, IsTerminal, Write};

use bustub::common::bustub_instance::BusTubInstance;
use bustub::common::result_writer::SimpleStreamWriter;

fn main() {
    let db = BusTubInstance::new(128);
    db.generate_mock_table();
    db.generate_test_table();
    let interactive = io::stdin().is_terminal();
    if interactive {
        println!("Welcome to the BusTub shell! Statements end with `;`. \\help lists the commands, \\q quits.");
    }
    let execute = |sql: &str| run(&db, sql);
    let stdin = io::stdin();
    let mut sql = String::new();
    loop {
        if interactive {
            print!("{}", if sql.is_empty() { "bustub> " } else { "   ...> " });
            io::stdout().flush().ok();
        }
        let mut line = String::new();
        if stdin.lock().read_line(&mut line).unwrap_or(0) == 0 {
            break;
        }
        let trimmed = line.trim();
        if sql.is_empty() && trimmed == "\\q" {
            break;
        }
        if sql.is_empty() && trimmed.starts_with('\\') {
            command(&db, trimmed);
            continue;
        }
        sql.push_str(&line);
        if trimmed.ends_with(';') {
            execute(sql.trim());
            sql.clear();
        }
    }
}

/// Runs a backslash command and prints its answer.
fn command(db: &BusTubInstance, text: &str) {
    let mut out = String::new();
    match db.execute_sql(text, &mut SimpleStreamWriter::new(&mut out, false, "\t"), None) {
        Ok(_) => print!("{out}"),
        Err(e) => println!("{e}"),
    }
}

/// Runs `sql` outside any transaction and prints the rows, or the error.
fn run(db: &BusTubInstance, sql: &str) {
    let mut out = String::new();
    match db.execute_sql(sql, &mut SimpleStreamWriter::new(&mut out, false, "\t"), None) {
        Ok(true) => print!("{out}"),
        Ok(false) => println!("{out}(the statement failed)"),
        Err(e) => println!("{e}"),
    }
}
