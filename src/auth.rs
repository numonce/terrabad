use reqwest::header::{HeaderMap, HeaderValue, COOKIE};
use reqwest::ClientBuilder;
use serde::Deserialize;
use std::collections::HashMap;
use std::error::Error;
#[derive(Deserialize, Debug)]
pub struct TokenData {
    pub data: Token,
}

#[derive(Deserialize, Debug)]
pub struct Token {
    pub ticket: String,
    #[serde(rename = "CSRFPreventionToken")]
    pub csrf: String,
}
pub async fn get_token(
    username: &mut String,
    password: &String,
    url: &String,
) -> Result<HeaderMap, Box<dyn Error>> {
    username.push_str("@pam");
    let mut json_data = HashMap::new();
    let user_slice = &username[..];
    let pass_slice = &password[..];
    json_data.insert("username", user_slice);
    json_data.insert("password", pass_slice);

    let client = ClientBuilder::new()
        .danger_accept_invalid_certs(true)
        .build()?;
    let url = format!("{}/api2/json/access/ticket", &url);
    let text = client
        .post(url)
        .json(&json_data)
        .send()
        .await?
        .text()
        .await?;
    let token: TokenData = serde_json::de::from_str::<TokenData>(&text)?;
    let new_cookie = format!("PVEAuthCookie={}", token.data.ticket);
    let mut headers = HeaderMap::new();
    headers.insert(COOKIE, HeaderValue::from_str(new_cookie.as_str())?);
    headers.insert(
        "Csrfpreventiontoken",
        HeaderValue::from_str(token.data.csrf.as_str())?,
    );
    Ok(headers)
}
