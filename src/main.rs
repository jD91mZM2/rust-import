#[macro_use] extern crate clap;
extern crate quote;
extern crate syn;

use clap::{App, Arg};
use quote::ToTokens;
use std::env;
use std::fs::OpenOptions;
use std::io::prelude::*;
use std::io::{self, SeekFrom};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use syn::{Item, ItemUse};

fn main() {
    let matches = App::new(crate_name!())
        .author(crate_authors!())
        .version(crate_version!())
        .arg(Arg::with_name("file")
            .help("The file to alter"))
        .arg(Arg::with_name("path")
            .help("The path to add"))
        .arg(Arg::with_name("print")
            .help("Print all existing import AST")
            .short("p")
            .long("print"))
        .get_matches();

    let print = matches.is_present("print");
    let path = matches.value_of("path");
    let file = matches.value_of("file").map(|s| PathBuf::from(s)).or_else(|| {
        let mut path = env::current_dir().ok()?;
        while !path.join("Cargo.toml").exists() {
            if !path.pop() {
                return None;
            }
        }
        path.push("src");
        path.push("main.rs");
        Some(path)
    });

    if file.is_none() {
        eprintln!("no path specified and main.rs could not be found");
        return;
    }
    let file = file.unwrap();

    let mut file = match OpenOptions::new().read(true).write(true).open(file) {
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
        let first_use = syntax.items.iter().position(|item| {
            if let Item::Use(_) = *item { true } else { false }
        });
        if first_use.is_none() {
            unimplemented!("No existing `use`. This isn't implemented yet.");
        }
        let (before, imports) = syntax.items.split_at(first_use.unwrap());
        let first_not = imports.iter().position(|item| {
            if let Item::Use(_) = *item { false } else { true }
        });
        let (imports, after) = if let Some(first_not) = first_not {
            imports.split_at(first_not)
        } else {
            (imports, &[] as &[syn::Item])
        };

        let mut imports = imports.to_vec();

        if let Some(path) = path {
            imports.push(Item::Use(path));
        }

        let mut result = Vec::with_capacity(before.len() + imports.len() + after.len());
        result.extend_from_slice(&before);
        result.extend_from_slice(&imports);
        result.extend_from_slice(&after);

        if print {
            for item in imports {
                if let Item::Use(import) = item {
                    println!("{}", import.into_tokens());
                } else { unreachable!(); }
            }
        }

        result
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
