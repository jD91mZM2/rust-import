# rust-import

First attempt at making an importing utility for Rust.

## Why?

The goal is to get some sort of auto-importing to Rust.  
Having a reliable way to add an import statement would greatly help with this.

## Goals

 - [ ] Preserve formatting (Discussion in #3)
 - [ ] Sort imports
 - [x] Actually make it work somewhat
 - [x] Basic auto-importing
 - [x] Group imports
 - ~~[x] Add missing `extern crate`s~~

## How do I use it?

**Abslutely not**. This is not in a usable state yet.

## Auto import?

I did have a plan to auto-import stuff, but I got multiple recommendations to leave that to the RLS.  
[Reddit thread](https://redd.it/7oibl7)

There is some basic auto importing with the `-a` option that asks Cargo what to import.
