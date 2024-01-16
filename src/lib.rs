#![allow(dead_code)]
#![doc = include_str!("../README.md")]

mod response;

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

#[derive(Debug, Clone)]
/// a collection of nix packages that results from a search
pub struct NixPackageCollection {
    pub packages: Vec<NixPackage>,
}

#[derive(Debug, Error)]
/// the possible errors that can happen in this library
pub enum NixSearchError {
    #[error("ureq (an http library) encountered an error: {source}")]
    UreqError {
        #[from]
        source: ureq::Error,
    },
    #[error("serde_json (used to parse json) encounted an unexpected error: {source}")]
    DeserializationError {
        #[from]
        source: serde_json::Error,
    },
    #[error("the elastic search endpoint had a server error")]
    ElasticSearchError {
        error: ElasticSearchResponseError,
        status: i64,
    },
}

/// **USE THIS**: This is where you define what parameterizes your search
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
                Self::prefix().join(&format!("{}nixos-{channel}", Self::ELASTIC_PREFIX))?
            }
            SearchWithin::Flakes => {
                Self::prefix().join(&format!("{}group-manual", Self::ELASTIC_PREFIX))?
            }
        }
        .join("/_search")
    }

    /// Search nix packages for your query
    pub fn send(&self) -> Result<NixPackageCollection, NixSearchError> {
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

        dbg!(res.status());

        let read = serde_json::from_reader::<_, response::SearchResponse>(res.into_reader())?;

        match read {
            response::SearchResponse::Error { error, status } => {
                Err(NixSearchError::ElasticSearchError { error, status })
            }
            response::SearchResponse::Success { packages } => Ok(NixPackageCollection { packages }),
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
        let multi_match_name = format!("multi_match_{}", self.search.replace(" ", "_"));
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
            .chain(self.search.split(" ").map(|split| {
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

        return json!({
            "dis_max":  {
                "tie_breaker": 0.7,
                "queries": queries,
            }
        });
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
