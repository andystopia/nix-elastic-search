use serde::{Deserialize, Serialize};

use crate::NixSearchError;

// Response is the format for an ElasticSearch API response.
// If the request was successful, only `Hits` will be populated.
// if the request failed, `Error` and `Status` will both be set, and `Hits` will be empty.pub struct Response {

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(from = "InnerSearchResponse")]
pub(crate) enum SearchResponse {
    Error {
        error: ElasticSearchResponseError,
        status: i64,
    },
    Success {
        packages: Vec<NixPackage>,
    },
}

impl From<InnerSearchResponse> for SearchResponse {
    fn from(value: InnerSearchResponse) -> Self {
        match value.error.zip(value.status) {
            Some((error, status)) => SearchResponse::Error { error, status },
            None => SearchResponse::Success {
                packages: value.hits.hits.into_iter().map(|h| h.source).collect(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct InnerSearchResponse {
    error: Option<ElasticSearchResponseError>,
    status: Option<i64>,
    hits: Hits,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ElasticSearchResponseError {
    #[serde(rename = "type")]
    type_field: String,
    reason: String,
    resource: ElasticSearchResponseErrorResource,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ElasticSearchResponseErrorResource {
    #[serde(rename = "type")]
    type_field: String,
    id: String,
}

pub struct ErrorResource {}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Hits {
    pub hits: Vec<Hit>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Hit {
    #[serde(rename = "_id")]
    pub id: String,
    // #[serde(rename = "_index")]
    // pub index: String,
    // #[serde(rename = "_score")]
    // pub score: f64,
    #[serde(rename = "_source")]
    pub source: NixPackage,
    // #[serde(rename = "_type")]
    // pub type_field: String,
    // #[serde(default)]
    // pub matched_queries: Vec<String>,
    // pub sort: (f64, String, String),
}

/// The primary thing that describes a Nix Package.
///
/// Constructed from succesful queries of the nix search elastic
/// search api, allows for reading of package attributes.
#[derive(Default, Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NixPackage {
    pub package_attr_name: String,
    pub package_attr_set: Option<String>,
    pub package_default_output: Option<String>,
    pub package_description: Option<String>,
    pub package_homepage: Vec<String>,
    pub package_license: Vec<PackageLicense>,
    pub package_license_set: Vec<String>,
    pub package_maintainers: Vec<PackageMaintainer>,
    pub package_maintainers_set: Vec<String>,
    pub package_outputs: Vec<String>,
    pub package_platforms: Vec<String>,
    pub package_pname: String,
    pub package_position: Option<String>,
    pub package_programs: Vec<String>,
    pub package_pversion: String,
    pub package_system: Option<String>,
    #[serde(rename = "type")]
    pub type_field: String,
}

#[cfg(feature = "version-search")]
impl NixPackage {
    /// WARNING: THIS CALLS A WEB SCRAPER. USE RESPONSIBLY.
    /// IF your'e doing a lot of searching, you should not
    /// be querying this method eagerly. It's going to be slow,
    /// and as far as I can tell, this person hosts this
    /// site out of the goodness of their heart for the Nix
    /// community to enjoy. Please have friction to using
    /// this method in your applications.
    pub fn all_versions(&self) -> Result<Vec<crate::PackageVersion>, NixSearchError> {
        crate::version::lookup_package_versions(&self.package_pname)
    }
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Eq, Hash)]
pub struct PackageLicense {
    #[serde(rename = "fullName")]
    pub full_name: String,
    pub url: Option<String>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Eq, Hash)]
#[serde(default)]
pub struct PackageMaintainer {
    pub email: Option<String>,
    pub name: Option<String>,
    // #[serde(flatten)]
    // pub extras: HashMap<String, serde_json::Value>,
}
