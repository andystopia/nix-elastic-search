# Nix Search for Rust

This is an (unfortunate, and certainly less well written) rewrite of [nix-search-cli](https://github.com/peterldowns/nix-search-cli),
which is meant to be used in library form in rust for seraching nixpkgs, similar
to how [nix search](search.nixos.org) on the web works.

This is NOT my original work. This is a derivative work of them. I did write the Rust code, but it's really just 
a port of their existing work. I unfortunately found this easier than bundling the existing binary or linking to it, 
so I can depend on it directly. I think the work they did is amazing. 


## Usage

```rust
use nix_elastic_search::{Query, SearchWithin, MatchSearch};

let query = Query {
    max_results: 10,
    search_within: SearchWithin::Channel("23.11".to_owned()),

    search: Some(MatchSearch {
        search: "gleam".to_owned(),
    }),
    program: None,
    name: None,
    version: None,
    query_string: None,
};

assert!(query.send().is_ok());
```
