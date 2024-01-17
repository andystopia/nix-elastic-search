use scraper::Selector;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::NixSearchError;

/// Represents verbose version information
/// for a chosen package.
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageVersion {
    pub version: String,
    pub revision: String,
    pub date: String,
}

/// Can lookup a specific package's old versions.
///
/// THIS IS A WEB SCRAPER: USE RESPONSIBLY.
///
/// As far as I can tell this scrapes a website hosted
/// by some guy out of the goodness of his own heart.
/// Please don't abuse it by pairing it up with search
/// and querying for every package, every version. Please
/// have some friction or do so lazily.
///
/// that said, unintuitively, you should call this function with
/// package_pname, anything else won't yield results.
pub fn lookup_package_versions(
    package_name: &str,
) -> Result<Vec<PackageVersion>, crate::NixSearchError> {
    let url = Url::parse_with_params(
        "https://lazamar.co.uk/nix-versions/",
        [("channel", "nixpkgs-unstable"), ("package", package_name)],
    )
    .map_err(|e| NixSearchError::InvalidPackageNameError {
        package_name: package_name.to_owned(),
        source: e,
    })?;

    let site = ureq::get(&url.to_string()).call()?;

    let site_text = site
        .into_string()
        .map_err(|e| NixSearchError::ErrorReadingVersionBody {
            package_name: package_name.to_owned(),
            source: e,
        })?;

    let parsed = scraper::Html::parse_document(&site_text);

    let select = Selector::parse("html > body > section > table > tbody").unwrap();
    let mut parse = parsed.select(&select);

    let element = parse
        .next()
        .ok_or(NixSearchError::MissingTableForVersions)?;
    let row_selector = Selector::parse("tr").unwrap();

    let mut versions = Vec::new();
    for row in element.select(&row_selector) {
        let version = row.text().nth(1).map(ToOwned::to_owned);
        let revision = row.text().nth(2).map(ToOwned::to_owned);
        let date = row.text().nth(3).map(ToOwned::to_owned);

        if let Some(((version, revision), date)) = version.zip(revision).zip(date) {
            versions.push(PackageVersion {
                version,
                revision,
                date,
            });
        }
    }
    Ok(versions)
}
