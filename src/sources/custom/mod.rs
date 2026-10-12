//! Formats whose structure is too bespoke for generic xml/csv/attributes
//! (paginated HTML listings, authenticated crawls, ...), grouped here apart
//! from the simple field-selector formats. Each still implements
//! `FormatParser` directly for its own config type (see `sources::mod`'s
//! `FormatConfig::parser()` boundary match) - there's no dispatch in this
//! module itself.

mod auth_crawl1;
mod html_listing1;
