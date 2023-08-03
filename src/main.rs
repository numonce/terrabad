use clap::{Arg, Command};
use reqwest::{
    blocking::ClientBuilder,
    header::{HeaderMap, HeaderValue, COOKIE},
};
use serde::Deserialize;
use std::{collections::HashMap, error::Error};

#[derive(Deserialize)]
struct Data {
    data: Token,
}

#[derive(Deserialize)]
struct Token {
    ticket: String,
    #[serde(rename = "CSRFPreventionToken")]
    csrf: String,
}

fn main() -> Result<(), Box<dyn Error>> {
    let app = Command::new("terrabad")
        .arg(
            Arg::new("url")
                .long("url")
                .short('U')
                .help("url of the Proxmox host")
                .required(true),
        )
        .arg(
            Arg::new("Username")
                .long("user")
                .short('u')
                .help("Username for proxmox auth")
                .required(true),
        )
        .arg(
            Arg::new("Password")
                .long("password")
                .short('p')
                .help("Password for proxmox auth")
                .required(true),
        )
        .arg(
            Arg::new("Action")
                .long("Action")
                .short('a')
                .help("Clone, etc...")
                .required(true)
                .value_parser(["Clone"]),
        )
        .arg(
            Arg::new("Name")
                .long("name")
                .short('n')
                .requires("Action")
                .help("Name of the node"),
        )
        .arg(
            Arg::new("Source")
                .long("source")
                .short('s')
                .requires("Action")
                .help("Source template VMID for action."),
        )
        .arg(
            Arg::new("Destination")
                .long("destination")
                .short('d')
                .requires("Action")
                .help("Destination template VMID for action."),
        )
        .get_matches();
    let action = app.get_one::<String>("Action").unwrap();
    let url = app.get_one::<String>("url").unwrap();
    let username = app.get_one::<String>("Username").unwrap();
    let password = app.get_one::<String>("Password").unwrap();
    let user_realm = format!("{}@pam", username);
    let token = get_token(&user_realm, password, url)?;
    if action.to_owned() == "Clone".to_string() {
        let name = app.get_one::<String>("Name").unwrap();
        let src = app.get_one::<String>("Source").unwrap();
        let dst = app.get_one::<String>("Destination").unwrap();
        create_clone(token, name, url, src, dst)?;
    }
    Ok(())
}

fn get_token(username: &String, password: &String, url: &String) -> Result<Data, Box<dyn Error>> {
    let mut json_data = HashMap::new();
    let user_slice = &username[..];
    let pass_slice = &password[..];
    json_data.insert("username", user_slice);
    json_data.insert("password", pass_slice);

    let client = ClientBuilder::new()
        .danger_accept_invalid_certs(true)
        .build()?;
    let url = format!("{}/api2/json/access/ticket", &url);
    let text = client.post(url).json(&json_data).send()?.text()?;
    let token: Data = serde_json::de::from_str::<Data>(&text)?;

    Ok(token)
}

fn create_clone(
    token: Data,
    name: &String,
    url: &String,
    src: &String,
    dst: &String,
) -> Result<(), Box<dyn Error>> {
    let client = ClientBuilder::new()
        .danger_accept_invalid_certs(true)
        .build()?;
    let new_cookie = format!("PVEAuthCookie={}", token.data.ticket);
    let mut headers = HeaderMap::new();
    headers.insert(COOKIE, HeaderValue::from_str(new_cookie.as_str())?);
    headers.insert(
        "Csrfpreventiontoken",
        HeaderValue::from_str(token.data.csrf.as_str())?,
    );
    let mut json_data = HashMap::new();
    json_data.insert("newid", dst.clone().to_owned());
    json_data.insert("node", name.clone().to_owned());
    json_data.insert("vmid", src.clone().to_owned());
    let url = format!("{}/api2/json/nodes/{}/qemu/{}/clone", url, name, src);
    let text = client
        .post(url)
        .headers(headers.clone())
        .json(&json_data)
        .send()?
        .text()?;
    println!("{:?}", text);
    Ok(())
}
