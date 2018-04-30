#[macro_use] extern crate clap;
#[macro_use] extern crate serde_derive;
extern crate failure;
extern crate quote;
extern crate serde_json;
extern crate syn;
extern crate take_mut;

use clap::{App, Arg};
use quote::ToTokens;
use std::{
    fs::OpenOptions,
    io::{self, prelude::*, SeekFrom},
    path::Path,
    process::{Command, Stdio}
};
use syn::{punctuated::Punctuated, Item, ItemUse, UseTree};

mod compile;

fn main() {
    let matches = App::new(crate_name!())
        .author(crate_authors!())
        .version(crate_version!())
        .arg(Arg::with_name("file")
            .help("The file to alter")
            .required(true)
            .multiple(true))
        .arg(Arg::with_name("import")
            .help("The import path to add")
            .short("i")
            .multiple(true)
            .number_of_values(1))
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
        .arg(Arg::with_name("sort")
            .help("Sort all imports alphabetically")
            .short("s")
            .long("sort"))
        .get_matches();

    let filenames = matches.values_of("file").unwrap();
    let imports   = matches.values_of("import");
    let print = matches.is_present("print");
    let auto  = matches.is_present("auto");
    let group = matches.is_present("group");
    let sort = matches.is_present("sort");

    let imports: Result<Vec<ItemUse>, syn::synom::ParseError> = imports
        .map(|imports| {
            imports
                .map(|path| {
                    let path = path.trim();
                    let mut string = String::with_capacity(4 + path.len() + 1);
                    string.push_str(path);
                    if !path.ends_with(';') {
                        string.push(';');
                    }

                    syn::parse_str(&string)
                        .or_else(|_| {
                            string.insert_str(0, "use ");
                            syn::parse_str(&string)
                        })
                })
                .collect()
        })
        .unwrap_or_else(|| Ok(Vec::new()));
    let imports = match imports {
        Ok(imports) => imports,
        Err(err) => {
            eprintln!("failed to parse path: {}", err);
            return;
        }
    };

    for filename in filenames {
        let filename = Path::new(filename);
        let mut file = match OpenOptions::new().read(true).write(true).open(&filename) {
            Ok(file) => file,
            Err(err) => {
                eprintln!("failed to open file {:?}: {}", filename, err);
                return;
            }
        };

        let mut src = String::new();
        if let Err(err) = file.read_to_string(&mut src) {
            eprintln!("error reading file: {}", err);
            return;
        }

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

        for path in &imports {
            uses.push(path.clone());
            modified = true;
        }

        if auto {
            match compile::compile(&filename) {
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
        if sort {
            modified = sort_uses(&mut uses);
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
}

fn is_extern_crate(item: &Item) -> bool {
    if let Item::ExternCrate(_) = *item { true } else { false }
}
fn is_use(item: &Item) -> bool {
    if let Item::Use(_) = *item { true } else { false }
}

trait UseStmt {
    fn as_tree(&self) -> &syn::UseTree;
    fn as_tree_mut(&mut self) -> &mut syn::UseTree;
    fn into_tree(self) -> syn::UseTree;
    fn should_group(&self, other: &Self) -> bool;
}

impl UseStmt for syn::ItemUse {
    fn as_tree(&self) -> &syn::UseTree { &self.tree }
    fn as_tree_mut(&mut self) -> &mut syn::UseTree { &mut self.tree }
    fn into_tree(self) -> syn::UseTree { self.tree }
    fn should_group(&self, other: &Self) -> bool {
        self.tree.should_group(&other.tree) &&
            self.attrs == other.attrs
    }
}
impl UseStmt for syn::UseTree {
    fn as_tree(&self) -> &syn::UseTree { self }
    fn as_tree_mut(&mut self) -> &mut syn::UseTree { self }
    fn into_tree(self) -> syn::UseTree { self }
    fn should_group(&self, other: &Self) -> bool {
        if let UseTree::Path(ref path) = *self {
            if let UseTree::Path(ref other_path) = *other {
                return path.ident == other_path.ident;
            }
        }
        false
    }
}

fn group_uses<T: UseStmt>(uses: Vec<T>) -> (bool, Vec<T>) {
    let mut grouped_uses: Vec<T> = Vec::with_capacity(uses.len());
    let mut modified = false;

    for item in uses {
        {
            let group = grouped_uses.iter_mut().find(|existing| item.should_group(existing));

            if let Some(group) = group {
                if let UseTree::Path(ref mut path) = *group.as_tree_mut() {
                    modified = true;

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
                        take_mut::take(&mut *path.tree, |tree| {
                            list.push(tree);
                            UseTree::Group(syn::UseGroup {
                                brace_token: syn::token::Brace::default(),
                                items: list
                            })
                        });
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
                take_mut::take(group, |mut group| {
                    let uses = group.items.into_iter().collect();
                    let (_, uses) = group_uses(uses);
                    group.items = uses.into_iter().collect();
                    group
                });
            }
        }
    }

    (modified, grouped_uses)
}

#[derive(PartialOrd, Ord, PartialEq, Eq)]
enum UseOrd {
    EmptyGroup,
    Glob,
    Ident(syn::Ident)
}
impl UseOrd {
    fn new(tree: &UseTree) -> Self {
        match *tree {
            UseTree::Path(ref path) => UseOrd::Ident(path.ident),
            UseTree::Name(ref name) => UseOrd::Ident(name.ident),
            UseTree::Rename(ref rename) => UseOrd::Ident(rename.ident),
            UseTree::Glob(_) => UseOrd::Glob,
            UseTree::Group(ref group) => {
                group.items.iter().next()
                    .map(UseOrd::new)
                    .unwrap_or(UseOrd::EmptyGroup)
            }
        }
    }
}

fn sort_inner(tree: &mut UseTree) -> bool {
    match *tree {
        UseTree::Path(ref mut path) => {
            sort_inner(&mut *path.tree)
        },
        UseTree::Group(ref mut group) => {
            let mut sorted = false;
            take_mut::take(&mut *group, |mut group| {
                let mut uses: Vec<_> = group.items.into_iter().collect();
                sorted = sort_uses(&mut uses);
                group.items = uses.into_iter().collect();
                group
            });

            sorted
        },
        _ => false
    }
}
fn sort_uses<T: UseStmt>(uses: &mut [T]) -> bool {
    let mut sorted = true;

    for item in uses.iter_mut() {
        sorted = sorted && !sort_inner(item.as_tree_mut());
    }

    if sorted {
        for slice in uses.windows(2) {
            if UseOrd::new(slice[0].as_tree()) > UseOrd::new(slice[1].as_tree()) {
                sorted = false;
                break;
            }
        }
    }

    if sorted {
        false
    } else {
        uses.sort_unstable_by_key(|item| UseOrd::new(item.as_tree()));
        true
    }
}
