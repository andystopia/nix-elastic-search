#![allow(dead_code)]

//! # Nix Search for Rust
//!
//! This is an (unfortunate, and certainly less well written) rewrite of [nix-search-cli](https://github.com/peterldowns/nix-search-cli),
//! which is meant to be used in library form in rust for seraching nixpkgs, similar
//! to how [nix search](search.nixos.org) on the web works.
//!
//! This is NOT my original work. This is a derivative work of them. I did write the Rust code, but it's really just
//! a port of their existing work. I unfortunately found this easier than bundling the existing binary or linking to it,
//! so I can depend on it directly. I think the work they did is amazing.
//!
//!
//! ## Usage
//!

/// ```rust
/// use nix_elastic_search::{Query, SearchWithin, MatchSearch};
///
/// let query = Query {
///     max_results: 10,
///     search_within: SearchWithin::Channel("23.11".to_owned()),
///
///     search: Some(MatchSearch {
///         search: "gleam".to_owned(),
///     }),
///     program: None,
///     name: None,
///     version: None,
///     query_string: None,
/// };
///
/// assert!(query.send().is_ok());
/// ```
///
/// ```rust
/// use nix_elastic_search::{Query, SearchWithin, MatchName};
///
/// let query = Query {
///     max_results: 10,
///     search_within: SearchWithin::Channel("23.11".to_owned()),
///     search: None,
///     program: None,
///     name: Some(MatchName { name: "rust".to_owned() }),
///     version: None,
///     query_string: None,
/// };
///
/// query.send().unwrap();
/// ```

#[derive(Debug)]
pub struct SerdeNixPackagePath {
    text: String,
}

impl SerdeNixPackagePath {
    pub fn new(text: String) -> Self {
        Self { text }
    }

    pub fn get_error_path(&self) -> String {
        let jd = &mut serde_json::Deserializer::from_str(&self.text);
        let result: Result<response::SearchResponse, _> = serde_path_to_error::deserialize(jd);

        match result {
            Ok(_) => "<no path found>".to_owned(),
            Err(err) => err.path().to_string(),
        }
    }
}
pub mod response;

use base64::prelude::*;
use response::{ElasticSearchResponseError, NixPackage};
use serde_json::json;
use thiserror::Error;
use url::Url;

/// chose whether to search in flakes or by channel
pub enum SearchWithin {
    /// should be something like 23.11 (not nixos-23.11)
    Channel(String),
    Flakes,
}

#[derive(Debug, Error)]
/// the possible errors that can happen in this library
pub enum NixSearchError {
    #[error("ureq (an http library) encountered an error: {source}")]
    UreqError {
        #[from]
        source: ureq::Error,
    },
    #[error("serde_json (used to parse json) encounted an unexpected error: {source}, at path: {}", path.get_error_path())]
    DeserializationError {
        path: SerdeNixPackagePath,
        source: serde_json::Error,
    },
    #[error("the elastic search endpoint had a server error")]
    ElasticSearchError {
        error: ElasticSearchResponseError,
        status: i64,
    },
}

/// **USE THIS**: This is where you define what parameterizes your search
/// note: multiple filters are allowed.

pub struct Query {
    pub max_results: u32,
    pub search_within: SearchWithin,

    pub search: Option<MatchSearch>,
    pub program: Option<MatchProgram>,
    pub name: Option<MatchName>,
    pub version: Option<MatchVersion>,
    pub query_string: Option<MatchQueryString>,
}

impl Query {
    fn prefix() -> Url {
        Url::parse("https://nixos-search-7-1733963800.us-east-1.bonsaisearch.net:443/").unwrap()
    }
    const ELASTIC_PREFIX: &'static str = "latest-*-";
    const USERNAME: &'static str = "aWVSALXpZv";
    const PASSWORD: &'static str = "X8gPHnzL52wFEekuxsfQ9cSh";

    fn get_url(&self) -> Result<Url, url::ParseError> {
        match &self.search_within {
            SearchWithin::Channel(channel) => {
                Self::prefix().join(&format!("/{}nixos-{channel}/", Self::ELASTIC_PREFIX))?
            }
            SearchWithin::Flakes => {
                Self::prefix().join(&format!("{}group-manual/", Self::ELASTIC_PREFIX))?
            }
        }
        .join("_search")
    }

    /// Search nix packages for your query
    pub fn send(&self) -> Result<Vec<NixPackage>, NixSearchError> {
        let res = ureq::post(
            self.get_url()
                // we could handle this, but really I think it's such a pain
                // and we can really verify it pretty decently anyways at compile time.
                .expect("failed to construct url; this indicates a bug in nix-elastic-search")
                .as_str(),
        )
        // gotta do the simple http authentication
        // since the library doesn't do it for me.
        .set(
            "Authorization",
            &BASE64_STANDARD.encode(format!("{}:{}", Self::USERNAME, Self::PASSWORD)),
        )
        .set("Content-Type", "application/json")
        .set("Accept", "application/json")
        .send_string(&self.payload().to_string())?;

        let text = res.into_string().unwrap();

        let read = match serde_json::from_str::<response::SearchResponse>(&text) {
            Ok(r) => r,
            Err(err) => {
                return Err(NixSearchError::DeserializationError {
                    path: SerdeNixPackagePath::new(text),
                    source: err,
                })
            }
        };

        match read {
            response::SearchResponse::Error { error, status } => {
                Err(NixSearchError::ElasticSearchError { error, status })
            }
            response::SearchResponse::Success { packages } => Ok(packages),
        }
    }
    fn payload(&self) -> serde_json::Value {
        let starting_payload = json!({
           "match": {
                "type": "package",
            }
        });

        let must = [
            Some(starting_payload),
            self.search.as_ref().map(MatchSearch::to_json),
            self.program.as_ref().map(MatchProgram::to_json),
            self.name.as_ref().map(MatchName::to_json),
            self.version.as_ref().map(MatchVersion::to_json),
            self.query_string.as_ref().map(MatchQueryString::to_json),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();

        json!({
            "from": 0,
            "size": self.max_results,
            "sort": {
                "_score":            "desc",
                "package_attr_name": "desc",
                "package_pversion":  "desc",
            },
            "query": {
                "bool": {
                    "must": must,
                },
            }
        })
    }
}

/// search by search string (like search.nixos.org -- I beleive)
pub struct MatchSearch {
    pub search: String,
}

impl MatchSearch {
    pub fn to_json(&self) -> serde_json::Value {
        let multi_match_name = format!("multi_match_{}", self.search.replace(' ', "_"));
        let initial_query = json!({
                "multi_match": {
                    "type":  "cross_fields",
                    "_name": multi_match_name,
                    "query": self.search,
                    "fields": [
                        "package_attr_name^9",
                        "package_attr_name.*^5.3999999999999995",
                        "package_programs^9",
                        "package_programs.*^5.3999999999999995",
                        "package_pname^6",
                        "package_pname.*^3.5999999999999996",
                        "package_description^1.3",
                        "package_description.*^0.78",
                        "package_pversion^1.3",
                        "package_pversion.*^0.78",
                        "package_longDescription^1",
                        "package_longDescription.*^0.6",
                        "flake_name^0.5",
                        "flake_name.*^0.3",
                        "flake_resolved.*^99",
                    ]
            }
        });

        let queries = std::iter::once(initial_query)
            .chain(self.search.split(' ').map(|split| {
                json!( {
                        "wildcard": {
                            "package_attr_name": {
                                "value": format!("*{}*", split),
                                "case_insensitive": true,
                            },
                        }
                    }
                )
            }))
            .collect::<Vec<_>>();

        json!({
            "dis_max":  {
                "tie_breaker": 0.7,
                "queries": queries,
            }
        })
    }
}

/// search by name
pub struct MatchName {
    pub name: String,
}

impl MatchName {
    pub fn to_json(&self) -> serde_json::Value {
        json!({
            "dis_max": {
                "tie_breaker": 0.7,
                "queries": [
                    {
                        "wildcard": {
                            "package_attr_name": {
                                "value": format!("{}*", self.name),
                            }
                        }
                    },
                    {
                        "match": {
                            "package_programs": self.name,
                        }
                    }
                ]
            }
        })
    }
}

/// search by programs
pub struct MatchProgram {
    pub program: String,
}

impl MatchProgram {
    pub fn to_json(&self) -> serde_json::Value {
        json!({
            "dis_max": {
                "tie_breaker": 0.7,
                "queries": [
                    {
                        "wildcard": {
                            "package_programs": {
                                "value": format!("{}*", self.program),
                            }
                        }
                    },
                    {
                        "match": {
                            "package_programs": self.program,
                        }
                    }
                ]
            }
        })
    }
}

/// search by versions
pub struct MatchVersion {
    pub version: String,
}

impl MatchVersion {
    pub fn to_json(&self) -> serde_json::Value {
        json!({
            "dis_max": {
                "tie_breaker": 0.7,
                "queries": [
                    {
                        "wildcard": {
                            "package_pversion": {
                                "value": format!("{}*", self.version),
                            }
                        }
                    },
                    {
                        "match": {
                            "package_pversion": self.version,
                        }
                    }
                ]
            }
        })
    }
}

/// search by query string
pub struct MatchQueryString {
    pub query_string: String,
}

impl MatchQueryString {
    pub fn to_json(&self) -> serde_json::Value {
        json!({
            "query_string": {
                "query": self.query_string,
            }
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_search() {
        let query = Query {
            max_results: 20,
            search_within: SearchWithin::Channel("23.11".to_owned()),

            search: None,
            program: None,
            name: Some(MatchName {
                name: "cargo".to_owned(),
            }),
            version: None,
            query_string: None,
        };

        let results = query.send().unwrap();

        let res = results
            .into_iter()
            .map(|p| {
                format!(
                    "{}: {}",
                    p.package_attr_name,
                    p.package_description.unwrap_or_default()
                )
            })
            .collect::<Vec<_>>();

        println!("{:?}", res);
    }

    #[test]
    fn test_search_name() {
        let query = Query {
            max_results: 10,
            search_within: SearchWithin::Channel("23.11".to_owned()),
            search: None,
            program: None,
            name: Some(MatchName {
                name: "rust".to_owned(),
            }),
            version: None,
            query_string: None,
        };

        query.send().unwrap();
    }

    #[test]
    fn test_url() {
        let query = Query {
            max_results: 10,
            search_within: SearchWithin::Channel("23.11".to_owned()),
            search: None,
            program: None,
            name: Some(MatchName {
                name: "rust".to_owned(),
            }),
            version: None,
            query_string: None,
        };

        let url = query.get_url().unwrap();
        eprintln!("{}", url);
    }
}
