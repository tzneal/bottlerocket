/// The schnauzer library can be used to render file- or string-based templates that contain
/// settings references, e.g. "foo-{{ settings.bar }}", and contains common helper functions for
/// use inside the templates.

#[macro_use]
extern crate log;

mod helpers;

use handlebars::Handlebars;
use serde::de::DeserializeOwned;
use snafu::ResultExt;
use std::collections::HashMap;
use std::path::Path;

pub mod error {
    use http::StatusCode;
    use snafu::Snafu;

    #[derive(Debug, Snafu)]
    #[snafu(visibility = "pub(super)")]
    pub enum Error {
        #[snafu(display("Error {}ing to {}: {}", method, uri, source))]
        APIRequest {
            method: String,
            uri: String,
            source: apiclient::Error,
        },

        #[snafu(display("Error {} when {}ing to {}: {}", code, method, uri, response_body))]
        APIResponse {
            method: String,
            uri: String,
            code: StatusCode,
            response_body: String,
        },

        #[snafu(display(
            "Error deserializing response as JSON from {} to '{}': {}",
            method,
            uri,
            source
        ))]
        ResponseJson {
            method: &'static str,
            uri: String,
            source: serde_json::Error,
        },
    }
}
pub use error::Error;
type Result<T> = std::result::Result<T, error::Error>;

/// Simple helper that extends the API client, abstracting the repeated request logic and
/// deserialization from JSON.
pub fn get_json<T, P, S1, S2, S3>(
    socket_path: P,
    uri: S1,
    // Query parameter name, query parameter value
    query: Option<(S2, S3)>,
) -> Result<T>
where
    T: DeserializeOwned,
    P: AsRef<Path>,
    S1: AsRef<str>,
    S2: AsRef<str>,
    S3: AsRef<str>,
{
    let mut uri = uri.as_ref().to_string();
    // Simplest query string handling; parameters come from API responses so we trust them enough
    // to send back
    if let Some((query_param, query_arg)) = query {
        uri = format!("{}?{}={}", uri, query_param.as_ref(), query_arg.as_ref());
    }

    let method = "GET";
    trace!("{}ing from {}", method, uri);
    let (code, response_body) = apiclient::raw_request(socket_path, &uri, method, None)
        .context(error::APIRequest { method, uri: &uri })?;

    if !code.is_success() {
        return error::APIResponse {
            method,
            uri,
            code,
            response_body,
        }
        .fail();
    }
    trace!("JSON response: {}", response_body);

    serde_json::from_str(&response_body).context(error::ResponseJson { method, uri })
}

/// Requests all settings from the API and wraps them in a "settings" key, so they can be used as
/// the data source for a handlebars templating call.
pub fn get_settings<P>(socket_path: P) -> Result<HashMap<String, model::Settings>>
where
    P: AsRef<Path>,
{
    debug!("Querying API for settings data");
    let settings: model::Settings =
        get_json(socket_path, "/settings", None as Option<(String, String)>)?;

    trace!("Settings values: {:?}", settings);

    // The following helps satisfy the Handlebars templating library.
    // The variables in the templates are prefixed with "settings"
    // {{ settings.foo.bar }} so we need to wrap the Settings struct in a map
    // with the key "settings".
    let mut wrapped_template_keys: HashMap<String, model::Settings> = HashMap::new();
    wrapped_template_keys.insert("settings".to_string(), settings);
    trace!("Final template keys map: {:?}", &wrapped_template_keys);

    Ok(wrapped_template_keys)
}

/// Build a handlebars template registry with our common helper functions.
pub fn build_template_registry() -> Result<handlebars::Handlebars<'static>> {
    let mut template_registry = Handlebars::new();
    // Strict mode will panic if a key exists in the template
    // but isn't provided in the data given to the renderer
    template_registry.set_strict_mode(true);

    template_registry.register_helper("base64_decode", Box::new(helpers::base64_decode));
    template_registry.register_helper("join_map", Box::new(helpers::join_map));
    template_registry.register_helper("default", Box::new(helpers::default));

    Ok(template_registry)
}
