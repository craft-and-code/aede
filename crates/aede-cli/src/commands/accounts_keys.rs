//! API key management; disclose a newly generated token only after publication.

use aede_core::accounts::{self, ApiKey};
use aede_core::json::Json;

use super::{Args, Res, ui};

const USAGE: &str = "aede accounts keys <name> [create <label> | revoke <id>] [--json]";

enum Operation<'a> {
    List,
    Create(&'a str),
    Revoke(&'a str),
}

fn parse<'a>(words: &'a [&str]) -> Result<(&'a str, Operation<'a>), &'static str> {
    match words {
        ["keys", name] => Ok((name, Operation::List)),
        ["keys", name, "create", label] => Ok((name, Operation::Create(label))),
        ["keys", name, "revoke", id] => Ok((name, Operation::Revoke(id))),
        _ => Err(USAGE),
    }
}

fn public_json(key: &ApiKey) -> Json {
    let mut row = Json::obj();
    row.set("id", key.id.clone().into());
    row.set("label", key.label.clone().into());
    row.set("created_at", key.created_at.into());
    row
}

pub(super) fn run(args: &Args, words: &[&str]) -> Res {
    let (username, operation) = parse(words)?;
    let path = accounts::accounts_path(&super::super::data_dir(args));
    let mut data = accounts::load(&path)?
        .ok_or("initialize the first administrator with aede accounts init <name>")?;
    let issued = match operation {
        Operation::List => None,
        Operation::Create(label) => {
            Some(data.create_api_key(username, label, aede_core::clock::now_seconds())?)
        }
        Operation::Revoke(id) => {
            data.revoke_api_key(username, id)?;
            None
        }
    };
    if !matches!(operation, Operation::List) {
        accounts::save(&data, &path)?;
    }
    if let Some((key, token)) = issued {
        if args.has("json") {
            let mut row = public_json(&key);
            row.set("token", token.into());
            println!("{}", row.to_string_pretty());
        } else {
            print_keys(&[&key]);
            println!("API key (shown once): {token}");
        }
    } else {
        let keys = data.api_keys(username)?;
        if args.has("json") {
            println!(
                "{}",
                Json::Arr(keys.iter().map(|key| public_json(key)).collect()).to_string_pretty()
            );
        } else {
            print_keys(&keys);
            if matches!(operation, Operation::Revoke(_)) {
                println!("API key revoked.");
            }
        }
    }
    Ok(())
}

fn print_keys(keys: &[&ApiKey]) {
    let mut table = ui::Table::new(&["ID", "Label", "Created (Unix seconds)"]);
    for key in keys {
        table.push(vec![
            key.id.clone(),
            key.label.clone(),
            key.created_at.to_string(),
        ]);
    }
    print!("{}", table.render());
}
