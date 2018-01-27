#[macro_use] extern crate clap;
#[macro_use] extern crate serde_derive;
extern crate failure;
extern crate quote;
extern crate serde_json;
extern crate syn;

use clap::{App, Arg};
use quote::ToTokens;
use std::fs::OpenOptions;
use std::io::prelude::*;
use std::io::{self, SeekFrom};
use std::path::Path;
use std::process::{Command, Stdio};
use syn::{Item, ItemUse};

mod compile;

fn main() {
    let matches = App::new(crate_name!())
        .author(crate_authors!())
        .version(crate_version!())
        .arg(Arg::with_name("file")
            .help("The file to alter")
            .required(true))
        .arg(Arg::with_name("path")
            .help("The path to add"))
        .arg(Arg::with_name("print")
            .help("Print all existing imports")
            .short("p")
            .long("print"))
        .arg(Arg::with_name("auto")
            .help("Fight the compiler, attempt at auto-import")
            .short("a")
            .long("auto-import"))
        .get_matches();

    let auto = matches.is_present("auto");
    let file_name = Path::new(matches.value_of("file").unwrap());
    let path = matches.value_of("path");
    let print = matches.is_present("print");

    let mut file = match OpenOptions::new().read(true).write(true).open(&file_name) {
        Ok(file) => file,
        Err(err) => {
            eprintln!("failed to open file: {}", err);
            return;
        }
    };

    let mut src = String::new();
    if let Err(err) = file.read_to_string(&mut src) {
        eprintln!("error reading file: {}", err);
        return;
    }

    let path = match path {
        None => None,
        Some(path) => {
            let mut string = String::with_capacity(4 + path.len() + 1);
            if !path.starts_with("use ") {
                string.push_str("use ");
            }
            string.push_str(path);
            if !path.ends_with(';') {
                string.push(';');
            }

            let syntax: ItemUse = match syn::parse_str(&string) {
                Ok(syntax) => syntax,
                Err(err) => {
                    eprintln!("failed to parse path: {}", err);
                    return;
                }
            };

            Some(syntax)
        }
    };

    let mut syntax = match syn::parse_file(&src) {
        Ok(syntax) => syntax,
        Err(err) => {
            eprintln!("failed to parse file: {}", err);
            return;
        }
    };
    let result = {
        // Count how much to allocate
        let crates_len = syntax.items.iter().filter(|&item| is_extern_crate(item)).count();
        let uses_len   = syntax.items.iter().filter(|&item| is_use(item)).count();

        // Separate crates and uses from the rest
        let mut crates = Vec::with_capacity(crates_len);
        let mut uses   = Vec::with_capacity(uses_len);

        syntax.items.retain(|item| {
            match *item {
                Item::ExternCrate(_) => { crates.push(item.clone()); false },
                Item::Use(_) => { uses.push(item.clone()); false },
                _ => true
            }
        });

        let mut modified = false;

        if let Some(path) = path {
            uses.push(Item::Use(path));
            modified = true;
        }

        if auto {
            match compile::compile(file_name) {
                Ok(imports) => {
                    if !imports.is_empty() {
                        uses.extend(imports.into_iter().map(|(_, item)| Item::Use(item)));
                        modified = true;
                    }
                }
                Err(err) => {
                    eprintln!("auto import failed: {}", err);
                    return;
                }
            }
        }

        let result = if modified {
            let mut result = Vec::with_capacity(crates.len() + uses.len() + syntax.items.len());
            result.extend_from_slice(&crates);
            result.extend_from_slice(&uses);
            result.extend_from_slice(&syntax.items);
            Some(result)
        } else {
            None
        };

        if print {
            for item in uses {
                if let Item::Use(import) = item {
                    println!("{}", import.into_tokens());
                } else { unreachable!(); }
            }
        }

        if !modified {
            return;
        }

        result.unwrap()
    };

    syntax.items = result;

    // TODO: https://github.com/dtolnay/syn/issues/294
    let child = Command::new("rustfmt")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let child = match child {
        Ok(child) => child,
        Err(err) => {
            eprintln!("failed to run command: {}", err);
            return;
        }
    };

    if let Err(err) = child.stdin.unwrap().write_all(syntax.into_tokens().to_string().as_bytes()) {
        eprintln!("failed to write to rustfmt: {}", err);
        return;
    }

    let mut error = String::new();
    if let Err(err) = child.stderr.unwrap().read_to_string(&mut error) {
        eprintln!("failed to read stderr: {}", err);
        return;
    }
    let error = error.trim();
    if !error.chars().all(char::is_whitespace) {
        eprintln!("rustfmt returned error: {}", error);
        return;
    }

    if let Err(err) = file.seek(SeekFrom::Start(0)).and_then(|_| file.set_len(0)) {
        eprintln!("failed to truncate file: {}", err);
        return;
    }
    if let Err(err) = io::copy(&mut child.stdout.unwrap(), &mut file) {
        eprintln!("error writing to file: {}", err);
    }
}

fn is_extern_crate(item: &Item) -> bool {
    if let Item::ExternCrate(_) = *item { true } else { false }
}
fn is_use(item: &Item) -> bool {
    if let Item::Use(_) = *item { true } else { false }
}
