// Copyright 2018-2021 the Deno authors. All rights reserved. MIT license.

use deno_core::error::generic_error;
use deno_core::error::type_error;
use deno_core::error::uri_error;
use deno_core::error::AnyError;
use deno_core::proc_macros::deno_op;
use deno_core::url::form_urlencoded;
use deno_core::url::quirks;
use deno_core::url::Url;
use deno_core::JsRuntime;
use deno_core::OpState;
use deno_core::ZeroCopyBuf;
use serde::Deserialize;
use serde::Serialize;
use std::panic::catch_unwind;
use std::path::PathBuf;

#[derive(Serialize)]
pub struct UrlParts {
  href: String,
  hash: String,
  host: String,
  hostname: String,
  origin: String,
  password: String,
  pathname: String,
  port: String,
  protocol: String,
  search: String,
  username: String,
}

/// Parse `UrlParseArgs::href` with an optional `UrlParseArgs::base_href`, or an
/// optional part to "set" after parsing. Return `UrlParts`.
#[deno_op]
pub fn op_url_parse(
  href: String,
  base_href: Option<String>,
  // If one of the following are present, this is a setter call. Apply the
  // proper `Url::set_*()` method after (re)parsing `href`.
  set_hash: Option<String>,
  set_host: Option<String>,
  set_hostname: Option<String>,
  set_password: Option<String>,
  set_pathname: Option<String>,
  set_port: Option<String>,
  set_protocol: Option<String>,
  set_search: Option<String>,
  set_username: Option<String>,
) -> Result<UrlParts, AnyError> {
  let base_url = base_href
    .as_ref()
    .map(|b| Url::parse(b).map_err(|_| type_error("Invalid base URL")))
    .transpose()?;
  let mut url = Url::options()
    .base_url(base_url.as_ref())
    .parse(&href)
    .map_err(|_| type_error("Invalid URL"))?;

  if let Some(hash) = set_hash.as_ref() {
    quirks::set_hash(&mut url, hash);
  } else if let Some(host) = set_host.as_ref() {
    quirks::set_host(&mut url, host).map_err(|_| uri_error("Invalid host"))?;
  } else if let Some(hostname) = set_hostname.as_ref() {
    quirks::set_hostname(&mut url, hostname)
      .map_err(|_| uri_error("Invalid hostname"))?;
  } else if let Some(password) = set_password.as_ref() {
    quirks::set_password(&mut url, password)
      .map_err(|_| uri_error("Invalid password"))?;
  } else if let Some(pathname) = set_pathname.as_ref() {
    quirks::set_pathname(&mut url, pathname);
  } else if let Some(port) = set_port.as_ref() {
    quirks::set_port(&mut url, port).map_err(|_| uri_error("Invalid port"))?;
  } else if let Some(protocol) = set_protocol.as_ref() {
    quirks::set_protocol(&mut url, protocol)
      .map_err(|_| uri_error("Invalid protocol"))?;
  } else if let Some(search) = set_search.as_ref() {
    quirks::set_search(&mut url, search);
  } else if let Some(username) = set_username.as_ref() {
    quirks::set_username(&mut url, username)
      .map_err(|_| uri_error("Invalid username"))?;
  }

  // TODO(nayeemrmn): Panic that occurs in rust-url for the `non-spec:`
  // url-constructor wpt tests: https://github.com/servo/rust-url/issues/670.
  let username = catch_unwind(|| quirks::username(&url)).map_err(|_| {
    generic_error(format!(
      "Internal error while parsing \"{}\"{}, \
       see https://github.com/servo/rust-url/issues/670",
      href,
      base_href
        .map(|b| format!(" against \"{}\"", b))
        .unwrap_or_default()
    ))
  })?;
  Ok(UrlParts {
    href: quirks::href(&url).to_string(),
    hash: quirks::hash(&url).to_string(),
    host: quirks::host(&url).to_string(),
    hostname: quirks::hostname(&url).to_string(),
    origin: quirks::origin(&url),
    password: quirks::password(&url).to_string(),
    pathname: quirks::pathname(&url).to_string(),
    port: quirks::port(&url).to_string(),
    protocol: quirks::protocol(&url).to_string(),
    search: quirks::search(&url).to_string(),
    username: username.to_string(),
  })
}

#[deno_op]
pub fn op_url_parse_search_params(args: String) -> Result<Vec<(String, String)>, AnyError> {
  let search_params: Vec<_> = form_urlencoded::parse(args.as_bytes())
    .into_iter()
    .map(|(k, v)| (k.as_ref().to_owned(), v.as_ref().to_owned()))
    .collect();
  Ok(search_params)
}

#[deno_op]
pub fn op_url_stringify_search_params(
  args: Vec<(String, String)>,
) -> Result<String, AnyError> {
  let search = form_urlencoded::Serializer::new(String::new())
    .extend_pairs(args)
    .finish();
  Ok(search)
}

/// Load and execute the javascript code.
pub fn init(isolate: &mut JsRuntime) {
  let files = vec![("deno:op_crates/url/00_url.js", include_str!("00_url.js"))];
  for (url, source_code) in files {
    isolate.execute(url, source_code).unwrap();
  }
}

pub fn get_declaration() -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("lib.deno_url.d.ts")
}
