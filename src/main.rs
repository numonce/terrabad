use clap::{Arg, ArgMatches, Command};
use reqwest::{
    blocking::ClientBuilder,
    header::{HeaderMap, HeaderValue, COOKIE},
};
use serde::Deserialize;
use std::{collections::HashMap, error::Error};

#[derive(Deserialize, Debug)]
struct Data {
    data: Token,
}

#[derive(Deserialize, Debug)]
struct Token {
    ticket: String,
    #[serde(rename = "CSRFPreventionToken")]
    csrf: String,
}

fn main() -> Result<(), Box<dyn Error>> {
    let app = Command::new("terrabad")
        .arg(
            Arg::new("Url")
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
                .help("clone, etc...")
                .required(true)
                .value_parser(["clone", "remove"]),
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
    match app.get_one::<String>("Action").unwrap().as_str() {
        "clone" => create_clone(app)?,
        "remove" => remove_vm(app)?,
        _ => panic!("asdfasdf"),
    }
    Ok(())
}

fn get_token(
    username: &mut String,
    password: &String,
    url: &String,
) -> Result<Data, Box<dyn Error>> {
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
    let text = client.post(url).json(&json_data).send()?.text()?;
    let token: Data = serde_json::de::from_str::<Data>(&text)?;

    Ok(token)
}

fn create_clone(app: ArgMatches) -> Result<(), Box<dyn Error>> {
    let name = app.get_one::<String>("Name").unwrap();
    let dst = app.get_one::<String>("Destination").unwrap();
    let src = app.get_one::<String>("Source").unwrap();
    let url = app.get_one::<String>("Url").unwrap();
    let username = app.get_one::<String>("Username").unwrap();
    let password = app.get_one::<String>("Password").unwrap();
    let token = get_token(&mut username.clone(), password, url)?;

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
    let n_url = format!("{}/api2/json/nodes/{}/qemu/{}/clone", url, name, src);
    let text = client
        .post(n_url)
        .headers(headers.clone())
        .json(&json_data)
        .send()?
        .text()?;
    println!("{}", text);
    Ok(())
}

fn remove_vm(app: ArgMatches) -> Result<(), Box<dyn Error>> {
    let client = ClientBuilder::new()
        .danger_accept_invalid_certs(true)
        .build()?;
    let name = app.get_one::<String>("Name").unwrap();
    let src = app.get_one::<String>("Source").unwrap();
    let url = app.get_one::<String>("Url").unwrap();
    let username = app.get_one::<String>("Username").unwrap();
    let password = app.get_one::<String>("Password").unwrap();
    let token = get_token(&mut username.clone(), password, url)?;
    let new_cookie = format!("PVEAuthCookie={}", token.data.ticket);
    let mut headers = HeaderMap::new();
    headers.insert(COOKIE, HeaderValue::from_str(new_cookie.as_str())?);
    headers.insert(
        "Csrfpreventiontoken",
        HeaderValue::from_str(token.data.csrf.as_str())?,
    );
    let n_url = format!("{}/api2/json/nodes/{}/qemu/{}", url, name, src);
    let text = client
        .delete(n_url)
        .headers(headers.clone())
        .send()?
        .text()?;
    println!("{:?}", text);
    Ok(())
}
