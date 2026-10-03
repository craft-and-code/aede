//! Local account administration without secrets in arguments or public output.

use std::io::{IsTerminal, Read};

use aede_core::accounts::{self, Accounts, Role};
use aede_core::json::Json;

use super::{Args, Res, ui};

#[path = "accounts_keys.rs"]
mod keys;

const USAGE: &str = "aede accounts [list | init <name> | create <name> <admin|user|auditor> | password <name> | role <name> <admin|user|auditor> | rename <name> <new-name> | enable <name> | disable <name> | revoke <name> | keys <name> [create <label> | revoke <id>]] [--password-stdin] [--json]";

fn password(args: &Args) -> Result<String, Box<dyn std::error::Error>> {
    if !args.has("password-stdin") || std::io::stdin().is_terminal() {
        return Err("supply a password using --password-stdin with redirected input; passwords are never command-line arguments".into());
    }
    let mut password = String::new();
    std::io::stdin()
        .take(accounts::MAX_PASSWORD_BYTES as u64 + 3)
        .read_to_string(&mut password)?;
    if password.ends_with('\n') {
        password.pop();
        if password.ends_with('\r') {
            password.pop();
        }
    }
    if password.len() > accounts::MAX_PASSWORD_BYTES || password.contains(['\r', '\n']) {
        return Err("provide one password line of at most 1024 UTF-8 bytes".into());
    }
    Ok(password)
}

/// Manage credentials using the OS owner's existing data-directory lock.
/// Read-only listing requires neither a catalog nor a writer lock.
pub fn accounts(args: &Args) -> Res {
    let words: Vec<&str> = args.positionals.iter().map(String::as_str).collect();
    let operation = words.first().copied().unwrap_or("list");
    let needs_password = matches!(operation, "init" | "create" | "password");
    if args.value("password-stdin").is_some() || args.has("password-stdin") && !needs_password {
        return Err("--password-stdin is a flag for init, create and password only".into());
    }
    if operation == "keys" {
        return keys::run(args, &words);
    }
    let count = match operation {
        "list" => {
            if words.is_empty() {
                0
            } else {
                1
            }
        }
        "init" | "password" | "enable" | "disable" | "revoke" => 2,
        "create" | "role" | "rename" => 3,
        _ => return Err(USAGE.into()),
    };
    if words.len() != count {
        return Err(USAGE.into());
    }
    let path = accounts::accounts_path(&super::data_dir(args));
    let mut data = accounts::load(&path)?;
    let now = aede_core::clock::now_seconds();
    if operation == "init" {
        if data.is_some() {
            return Err("accounts are already initialized; use create or password".into());
        }
        data = Some(Accounts::bootstrap(words[1], &password(args)?, now)?);
    } else if operation != "list" {
        let held = data.as_mut().ok_or(
            "initialize the first administrator with aede accounts init <name> --password-stdin",
        )?;
        match operation {
            "create" => held.create(
                words[1],
                &password(args)?,
                Role::parse(words[2]).ok_or("role must be admin, user or auditor")?,
                now,
            )?,
            "password" => held.set_password(words[1], &password(args)?, now)?,
            "role" => held.set_role(
                words[1],
                Role::parse(words[2]).ok_or("role must be admin, user or auditor")?,
                now,
            )?,
            "rename" => held.rename(words[1], words[2], now)?,
            "enable" | "disable" => held.set_enabled(words[1], operation == "enable", now)?,
            "revoke" => held.revoke(words[1], now)?,
            _ => return Err(USAGE.into()),
        }
    }
    if operation != "list" {
        accounts::save(data.as_ref().ok_or("accounts were not initialized")?, &path)?;
    }
    if args.has("json") {
        let rows = data.as_ref().map_or(&[][..], Accounts::all);
        println!(
            "{}",
            Json::Arr(
                rows.iter()
                    .map(|account| {
                        let mut row = Json::obj();
                        row.set("id", account.id.clone().into());
                        row.set("username", account.username.clone().into());
                        row.set("role", account.role.as_str().into());
                        row.set("enabled", account.enabled.into());
                        row.set("created_at", account.created_at.into());
                        row.set("updated_at", account.updated_at.into());
                        row
                    })
                    .collect()
            )
            .to_string_pretty()
        );
    } else if let Some(data) = data {
        let mut table = ui::Table::new(&["Login", "Role", "Access", "Personal owner"]);
        for account in data.all() {
            table.push(vec![
                account.username.clone(),
                account.role.as_str().into(),
                if account.enabled {
                    "enabled"
                } else {
                    "disabled"
                }
                .into(),
                account.id.clone(),
            ]);
        }
        print!("{}", table.render());
        if operation != "list" {
            println!("Account update saved; affected sessions and API keys are revoked.");
        }
    } else {
        println!(
            "No accounts configured. Initialize one with aede accounts init <name> --password-stdin."
        );
    }
    Ok(())
}
