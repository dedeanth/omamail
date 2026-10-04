//! Google Contacts for recipient suggestions: the account's saved contacts and
//! its "Other contacts" (people it has written to). Read-only, bounded, and
//! optional: a grant without the contacts scopes yields an empty list rather
//! than an error, so mail keeps working for accounts that declined them.
use reqwest::{Client, Url};
use serde_json::{Value, json};
use std::{sync::OnceLock, time::Duration};

const RESPONSE_LIMIT: usize = 8 * 1024 * 1024;
const TOTAL_LIMIT: usize = 16 * 1024 * 1024; // across every page of both listings
const DEADLINE: Duration = Duration::from_secs(60);
const MAX_CONTACTS: usize = 10000;
const MAX_PAGES: usize = 50;
static CLIENT: OnceLock<Result<Client, &'static str>> = OnceLock::new();

fn client() -> Result<&'static Client, &'static str> {
    CLIENT
        .get_or_init(|| {
            Client::builder()
                .https_only(true)
                .hickory_dns(true)
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .connect_timeout(Duration::from_secs(10))
                .timeout(Duration::from_secs(20))
                .build()
                .map_err(|_| "contacts_network_failed")
        })
        .as_ref()
        .map_err(|e| *e)
}

async fn body(mut response: reqwest::Response) -> Result<String, &'static str> {
    if response
        .content_length()
        .is_some_and(|size| size > RESPONSE_LIMIT as u64)
    {
        return Err("contacts_response_too_large");
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "contacts_network_failed")?
    {
        if chunk.len() > RESPONSE_LIMIT - bytes.len() {
            return Err("contacts_response_too_large");
        }
        bytes.extend_from_slice(&chunk);
    }
    String::from_utf8(bytes).map_err(|_| "contacts_invalid_response")
}

/// Name and addresses of one People API person, as `{name, email}` rows.
fn rows(person: &Value, out: &mut Vec<Value>) {
    let name = person["names"]
        .as_array()
        .and_then(|names| names.first())
        .and_then(|name| name["displayName"].as_str())
        .unwrap_or("");
    for address in person["emailAddresses"].as_array().into_iter().flatten() {
        if let Some(email) = address["value"].as_str()
            && out.len() < MAX_CONTACTS
        {
            out.push(json!({"name":name,"email":email}));
        }
    }
}

/// Every page of one listing. `Ok(false)` when the grant lacks its scope.
async fn listing(
    token: &str,
    url: &str,
    query: &[(&str, &str)],
    key: &str,
    out: &mut Vec<Value>,
    bytes: &mut usize,
) -> Result<bool, &'static str> {
    let origin = Url::parse(url).map_err(|_| "contacts_network_failed")?;
    let mut page_token = String::new();
    for _ in 0..MAX_PAGES {
        let mut url = origin.clone();
        url.query_pairs_mut().extend_pairs(query);
        if !page_token.is_empty() {
            url.query_pairs_mut().append_pair("pageToken", &page_token);
        }
        let response = client()?
            .get(url)
            .bearer_auth(token)
            .send()
            .await
            .map_err(|_| "contacts_network_failed")?;
        match response.status().as_u16() {
            401 => return Err("contacts_auth_refused"),
            // Scope not granted, or the People API is off in the client's
            // Cloud project: no contacts from this listing, nothing broken.
            403 => return Ok(false),
            status if !(200..300).contains(&status) => return Err("contacts_request_failed"),
            _ => (),
        }
        let text = body(response).await?;
        *bytes += text.len();
        if *bytes > TOTAL_LIMIT {
            return Err("contacts_response_too_large");
        }
        let payload: Value =
            serde_json::from_str(&text).map_err(|_| "contacts_invalid_response")?;
        for person in payload[key].as_array().into_iter().flatten() {
            rows(person, out);
        }
        match payload["nextPageToken"].as_str() {
            Some(next) if !next.is_empty() && next != page_token && out.len() < MAX_CONTACTS => {
                page_token = next.to_owned()
            }
            _ => return Ok(true),
        }
    }
    Ok(true)
}

pub async fn fetch(token: &str) -> Result<Value, &'static str> {
    tokio::time::timeout(DEADLINE, fetch_all(token))
        .await
        .map_err(|_| "contacts_timeout")?
}

async fn fetch_all(token: &str) -> Result<Value, &'static str> {
    let mut out = Vec::new();
    let mut bytes = 0;
    let saved = listing(
        token,
        "https://people.googleapis.com/v1/people/me/connections",
        &[
            ("personFields", "names,emailAddresses"),
            ("pageSize", "1000"),
        ],
        "connections",
        &mut out,
        &mut bytes,
    )
    .await?;
    let other = listing(
        token,
        "https://people.googleapis.com/v1/otherContacts",
        &[("readMask", "names,emailAddresses"), ("pageSize", "1000")],
        "otherContacts",
        &mut out,
        &mut bytes,
    )
    .await?;
    Ok(json!({"granted": saved || other, "contacts": out}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_take_the_display_name_and_every_address() {
        let mut out = Vec::new();
        rows(
            &json!({"names":[{"displayName":"Ada Lovelace"}],
                    "emailAddresses":[{"value":"ada@example.com"},{"value":"ada@work.example"}]}),
            &mut out,
        );
        rows(
            &json!({"emailAddresses":[{"value":"nameless@example.com"}]}),
            &mut out,
        );
        rows(&json!({"names":[{"displayName":"No Mail"}]}), &mut out);
        assert_eq!(
            out,
            vec![
                json!({"name":"Ada Lovelace","email":"ada@example.com"}),
                json!({"name":"Ada Lovelace","email":"ada@work.example"}),
                json!({"name":"","email":"nameless@example.com"}),
            ]
        );
    }
}
