//! Local account administration without secrets in arguments or public output.

use aede_core::accounts::{self, Accounts, Role};
use aede_core::json::Json;
use aede_core::store_lock::StoreLock;

use super::{Args, Res, ui};

#[path = "accounts_keys.rs"]
mod keys;
#[path = "accounts_password.rs"]
mod password_input;

const USAGE: &str = "aede accounts [list | init <name> | create <name> <admin|user|auditor> | password <name> | role <name> <admin|user|auditor> | rename <name> <new-name> | enable <name> | disable <name> | revoke <name> | keys <name> [create <label> | revoke <id>]] [--password-stdin] [--json]";

fn writer_lock(args: &Args) -> Result<Option<StoreLock>, Box<dyn std::error::Error>> {
    if crate::mutates_store_with_args("accounts", args) {
        Ok(Some(StoreLock::acquire(&super::data_dir(args))?))
    } else {
        Ok(None)
    }
}

/// Read and confirm masked terminal passwords, or explicit redirected input.
/// Reload credentials under the OS owner's data-directory lock after input.
/// Read-only listing requires neither a catalog nor a writer lock.
pub fn accounts(args: &Args) -> Res {
    let words: Vec<&str> = args.positionals.iter().map(String::as_str).collect();
    let operation = words.first().copied().unwrap_or("list");
    let needs_password = matches!(operation, "init" | "create" | "password");
    if args.value("password-stdin").is_some() || args.has("password-stdin") && !needs_password {
        return Err("--password-stdin is a flag for init, create and password only".into());
    }
    if operation == "keys" {
        let _lock = writer_lock(args)?;
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
    let password = if needs_password {
        // Reject obvious mistakes before requesting a secret. This snapshot
        // is only preflight: publication reloads under the writer lock below.
        accounts::login_name(words[1])?;
        let current = accounts::load(&path)?;
        if operation == "init" {
            if current.is_some() {
                return Err("accounts are already initialized; use create or password".into());
            }
        } else {
            let current = current
                .ok_or("initialize the first administrator with aede accounts init <name>")?;
            if operation == "create" {
                Role::parse(words[2]).ok_or("role must be admin, user or auditor")?;
                if current.find(words[1]).is_some() {
                    return Err("this login name is already in use".into());
                }
            } else if current.find(words[1]).is_none() {
                return Err("no account has this login name".into());
            }
        }
        Some(password_input::read(args)?)
    } else {
        None
    };
    let _lock = writer_lock(args)?;
    let mut data = accounts::load(&path)?;
    let now = aede_core::clock::now_seconds();
    if operation == "init" {
        if data.is_some() {
            return Err("accounts are already initialized; use create or password".into());
        }
        data = Some(Accounts::bootstrap(
            words[1],
            password.as_deref().ok_or("password entry is required")?,
            now,
        )?);
    } else if operation != "list" {
        let held = data
            .as_mut()
            .ok_or("initialize the first administrator with aede accounts init <name>")?;
        match operation {
            "create" => held.create(
                words[1],
                password.as_deref().ok_or("password entry is required")?,
                Role::parse(words[2]).ok_or("role must be admin, user or auditor")?,
                now,
            )?,
            "password" => held.set_password(
                words[1],
                password.as_deref().ok_or("password entry is required")?,
                now,
            )?,
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
        println!("No accounts configured. Initialize one with aede accounts init <name>.");
    }
    Ok(())
}
