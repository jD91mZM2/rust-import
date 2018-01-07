use failure::Error;
use serde_json;
use std::io::BufReader;
use std::io::prelude::*;
use std::path::Path;
use std::process::{Command, Stdio};
use syn::{self, ItemUse};

#[derive(Deserialize, Debug)]
struct Span {
    file_name: String,
    suggested_replacement: Option<String>
}
#[derive(Deserialize, Debug)]
struct Child {
    spans: Vec<Span>
}
#[derive(Deserialize, Debug)]
struct Message {
    children: Vec<Child>
}
#[derive(Deserialize, Debug)]
struct Output {
    message: Option<Message>
}

pub fn compile(file: &Path) -> Result<Vec<(String, ItemUse)>, Error> {
    let child = Command::new("cargo")
        .arg("rustc")
        .arg("--message-format")
        .arg("json")
        .stderr(Stdio::null())
        .stdout(Stdio::piped())
        .spawn()?;

    let mut imports: Vec<(String, ItemUse)> = Vec::new();

    let reader = BufReader::new(child.stdout.unwrap());

    for line in reader.lines() {
        let line = line?;

        let output: Output = serde_json::from_str(&line)?;
        if let Some(message) = output.message {
            for child in message.children {
                for span in child.spans {
                    if file == Path::new(&span.file_name) {
                        if let Some(replacement) = span.suggested_replacement {
                            let (import, add) = {
                                let trim = replacement.trim();
                                let import = syn::parse_str(trim)?;

                                (import, imports.iter().all(|&(ref import, _)| import.trim() != trim))
                            };
                            if add {
                                // ItemUse does not implement PartialEq.
                                // Can't push trimmed because it doesn't live long enough.
                                imports.push((replacement, import));
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(imports)
}
