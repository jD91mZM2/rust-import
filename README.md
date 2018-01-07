# rust-import

First attempt at making an importing utility for Rust.

## Why?

The goal is to get some sort of auto-importing to Rust.  
Having a reliable way to add an import statement would greatly help with this.

## Goals

 - [ ] Group imports
 - [ ] Preserve formatting (See [dtolnay/syn#294](https://github.com/dtolnay/syn/issues/294))
 - [ ] Sort imports
 - [x] Actually make it work somewhat
 - [x] Add missing `extern crate`s
 - [x] Basic auto-importing

## How do I use it?

**Abslutely not**. This is not in a usable state yet.

## Auto import?

I did have a plan to auto-import stuff, but I got multiple recommendations to leave that to the RLS.  
[Reddit thread](https://redd.it/7oibl7)
