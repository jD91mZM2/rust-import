# rust-import

First attempt at making an importing utility for Rust.

## Why?

The goal is to get some sort of auto-importing to Rust.  
Having a reliable way to add an import statement would greatly help with this.

## Goals

 - [ ] Automatically add imports
 - [ ] Group imports
 - [ ] Preserve formatting (See [dtolnay/syn#294](https://github.com/dtolnay/syn/issues/294))
 - [ ] Sort imports
 - [x] Actually make it work somewhat
 - [x] Add missing `extern crate`s

## How do I use it?

**Abslutely not**. This is not in a usable state yet.

## Auto import?

Here's how I think the auto-import should work:

  - There is a `-l` flag that takes arguments like `std::net::TcpStream,UdpSocket`, etc.
  - That means it learns about `std`, `std::net`, `std::net::TcpStream` and `std::net::UdpSocket`.
  - Then it goes through the code, looking for use statements.
    - Every time a block is seen, blocks += 1.
    - Every path gets expanded (globs get read from the learn).
    - Every path gets added to a list of all things, along with the level.
    - Every time an ident that isn't imported is seen, it tries to import one from the learned ones.
    - Every time a block is closed, blocks -= 1 and remove all the things from the list.

## Unanswered questions

How do you get each token in `syn`?  
Does that allow me to count how many blocks deep?  
Does that allow me to know which tokens are idents?
