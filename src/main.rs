#[macro_use] extern crate clap;
#[macro_use] extern crate serde_derive;
extern crate failure;
extern crate quote;
extern crate serde_json;
extern crate syn;

use clap::{App, Arg};
use quote::ToTokens;
use std::{
    fs::OpenOptions,
    io::{self, prelude::*, SeekFrom},
    mem,
    path::Path,
    process::{Command, Stdio}
};
use syn::{Item, ItemUse, UseTree, punctuated::Punctuated};

mod compile;

fn main() {
    let matches = App::new(crate_name!())
        .author(crate_authors!())
        .version(crate_version!())
        .arg(Arg::with_name("file")
            .help("The file to alter")
            .required(true))
        .arg(Arg::with_name("path")
            .help("The import path to add"))
        .arg(Arg::with_name("print")
            .help("Print all existing imports")
            .short("p")
            .long("print"))
        .arg(Arg::with_name("auto")
            .help("Fight the compiler, attempt to auto-import")
            .short("a")
            .long("auto-import"))
        .arg(Arg::with_name("group")
            .help("Group all imports into trees")
            .short("g")
            .long("group"))
        .get_matches();

    let file_name = Path::new(matches.value_of("file").unwrap());
    let path  = matches.value_of("path");
    let print = matches.is_present("print");
    let auto  = matches.is_present("auto");
    let group = matches.is_present("group");

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
    // Count how much to allocate
    let crates_len = syntax.items.iter().filter(|&item| is_extern_crate(item)).count();
    let uses_len   = syntax.items.iter().filter(|&item| is_use(item)).count();

    // Separate crates and uses from the rest
    let mut crates = Vec::with_capacity(crates_len);
    let mut uses   = Vec::with_capacity(uses_len);

    syntax.items.retain(|item| {
        match *item {
            Item::ExternCrate(ref inner) => { crates.push(inner.clone()); false },
            Item::Use(ref inner) =>         { uses.push(inner.clone());   false },
            _ => true
        }
    });

    let mut modified = false;

    if let Some(path) = path {
        uses.push(path);
        modified = true;
    }

    if auto {
        match compile::compile(file_name) {
            Ok(imports) => {
                if !imports.is_empty() {
                    uses.extend(imports.into_iter().map(|(_, item)| item));
                    modified = true;
                }
            }
            Err(err) => {
                eprintln!("auto import failed: {}", err);
                return;
            }
        }
    }
    if group {
        let (new_modified, new_uses) = group_uses(uses);
        modified = new_modified;
        uses = new_uses;
    }

    if print {
        for item in &uses {
            let mut tokens = quote::Tokens::new();
            item.to_tokens(&mut tokens);
            println!("{}", tokens);
        }
    }

    if !modified {
        return;
    }

    let mut result = Vec::with_capacity(crates.len() + uses.len() + syntax.items.len());
    result.extend(crates.into_iter().map(|item| Item::ExternCrate(item)));
    result.extend(uses.into_iter().map(|item| Item::Use(item)));
    result.extend(syntax.items);

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

trait AsUseTree {
    fn as_tree(&self) -> &syn::UseTree;
    fn as_tree_mut(&mut self) -> &mut syn::UseTree;
    fn into_tree(self) -> syn::UseTree;
}

impl AsUseTree for syn::ItemUse {
    fn as_tree(&self) -> &syn::UseTree { &self.tree }
    fn as_tree_mut(&mut self) -> &mut syn::UseTree { &mut self.tree }
    fn into_tree(self) -> syn::UseTree { self.tree }
}
impl AsUseTree for syn::UseTree {
    fn as_tree(&self) -> &syn::UseTree { self }
    fn as_tree_mut(&mut self) -> &mut syn::UseTree { self }
    fn into_tree(self) -> syn::UseTree { self }
}

fn group_uses<T: AsUseTree>(uses: Vec<T>) -> (bool, Vec<T>) {
    let mut grouped_uses: Vec<T> = Vec::with_capacity(uses.len());
    let mut modified = false;

    for item in uses {
        {
            let mut group = None;
            if let UseTree::Path(ref path) = *item.as_tree() {
                group = grouped_uses.iter_mut().find(|item| {
                    if let UseTree::Path(ref path2) = *item.as_tree() {
                        if path.ident == path2.ident {
                            return true;
                        }
                    }
                    false
                });
            }
            if let Some(group) = group {
                if let UseTree::Path(ref mut path) = *group.as_tree_mut() {
                    modified = true;
                    println!("Merging with: {:?}", path.ident);

                    let value = if let UseTree::Path(path) = item.into_tree() {
                        *path.tree
                    } else { unreachable!(); };

                    let values = if let UseTree::Group(group) = value {
                        group.items
                    } else {
                        let mut list = Punctuated::new();
                        list.push_value(value);
                        list
                    };

                    if let UseTree::Group(ref mut group) = *path.tree {
                        let mut list = &mut group.items;
                        for value in values {
                            list.push(value);
                        }
                    } else {
                        let mut list = values;
                        // temporary ownership
                        let tree = mem::replace(&mut *path.tree, unsafe { mem::uninitialized() });
                        list.push(tree);

                        mem::forget(mem::replace(&mut *path.tree, UseTree::Group(syn::UseGroup {
                            brace_token: syn::token::Brace::default(),
                            items: list
                        })));
                    };
                } else { unreachable!(); }
                continue;
            }
        }
        grouped_uses.push(item);
    }

    for item in &mut grouped_uses {
        if let UseTree::Path(ref mut path) = *item.as_tree_mut() {
            if let UseTree::Group(ref mut group) = *path.tree {
                // temporary ownership
                let mut group2 = mem::replace(group, unsafe { mem::uninitialized() });

                let mut uses = group2.items.into_iter().collect();
                let (_, uses) = group_uses(uses);
                group2.items = uses.into_iter().collect();

                mem::forget(mem::replace(group, group2));
            }
        }
    }

    (modified, grouped_uses)
}
