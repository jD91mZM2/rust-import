#[macro_use] extern crate clap;
extern crate quote;
extern crate syn;

use clap::{App, Arg};
use std::env;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use syn::{Item, ItemUse};
use quote::ToTokens;

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

    let mut src = String::new();
    if let Err(err) = read(&file, &mut src) {
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
}
fn read<P: AsRef<Path>>(file: P, src: &mut String) -> Result<(), std::io::Error> {
    File::open(file)?.read_to_string(src).map(|_| ())
}
