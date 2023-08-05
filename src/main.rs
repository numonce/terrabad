use clap::{Arg, ArgMatches, Command};
use reqwest::{
    blocking::ClientBuilder,
    header::{HeaderMap, HeaderValue, COOKIE},
};
use serde::Deserialize;
use serde_json::Map;
use serde_json::Value;
use std::{collections::HashMap, error::Error};

#[derive(Deserialize, Debug)]
struct TokenData {
    data: Token,
}

#[derive(Deserialize, Debug)]
struct Token {
    ticket: String,
    #[serde(rename = "CSRFPreventionToken")]
    csrf: String,
}
#[derive(Deserialize, Debug)]
struct UPIDData {
    data: String,
}
#[derive(Deserialize, Debug)]
struct JobData {
    data: Job,
}

#[derive(Deserialize, Debug)]
struct Job {
    exitstatus: String,
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
                .value_parser(["clone", "remove", "bulk_clone", "bulk_destroy"]),
        )
        .arg(
            Arg::new("Name")
                .long("name")
                .short('n')
                .required(true)
                .help("Name of the node"),
        )
        .arg(
            Arg::new("Source")
                .long("source")
                .short('s')
                .required(true)
                .help("Source template VMID for action."),
        )
        .arg(
            Arg::new("Destination")
                .long("destination")
                .short('d')
                .requires("Action")
                .help("Destination template VMID for action."),
        )
        .arg(
            Arg::new("Min")
                .long("min")
                .short('m')
                .requires("Action")
                .help("First VMID for range. Needed for bulk actions.")
                .required_if_eq_any([("Action", "bulk_clone")]),
        )
        .arg(
            Arg::new("Max")
                .long("max")
                .short('M')
                .requires("Action")
                .help("Last VMID for range. Needed for bulk actions.")
                .required_if_eq_any([("Action", "bulk_clone")]),
        )
        .arg(
            Arg::new("Clone_type")
                .long("clone_type")
                .short('t')
                .help("Type of clone. Can either be linked or full.")
                .default_value("linked")
                .value_parser(["linked", "full"]),
        )
        .get_matches();
    match app.get_one::<String>("Action").unwrap().as_str() {
        "clone" => create_clone(app)?,
        "remove" => remove_vm(app)?,
        "bulk_clone" => bulk_clone(app)?,
        "bulk_destroy" => bulk_destroy(app)?,
        _ => panic!("asdfasdf"),
    }
    Ok(())
}

fn get_token(
    username: &mut String,
    password: &String,
    url: &String,
) -> Result<TokenData, Box<dyn Error>> {
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
    let token: TokenData = serde_json::de::from_str::<TokenData>(&text)?;

    Ok(token)
}

fn create_clone(app: ArgMatches) -> Result<(), Box<dyn Error>> {
    let name = app.get_one::<String>("Name").unwrap();
    let dst = app.get_one::<String>("Destination").unwrap();
    let src = app.get_one::<String>("Source").unwrap();
    let url = app.get_one::<String>("Url").unwrap();
    let clone_type = app.get_one::<String>("Clone_type").unwrap();
    let username = app.get_one::<String>("Username").unwrap();
    let password = app.get_one::<String>("Password").unwrap();
    let token = get_token(&mut username.clone(), password, url)?;
    let full = match clone_type.as_str() {
        "linked" => false,
        "full" => true,
        _ => false,
    };
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
    let mut json_data = Map::new();
    json_data.insert("newid".to_string(), Value::String(dst.to_owned()));
    json_data.insert("node".to_string(), Value::String(name.to_owned()));
    json_data.insert(
        "vmid".to_string(),
        serde_json::Value::String(src.to_owned()),
    );
    json_data.insert("full".to_string(), Value::Bool(full));
    let n_url = format!("{}/api2/json/nodes/{}/qemu/{}/clone", url, name, src);
    let text = client
        .post(n_url)
        .headers(headers.clone())
        .json(&json_data)
        .send()?
        .text()?;
    let upid: UPIDData = serde_json::de::from_str::<UPIDData>(text.as_str())?;
    finished(headers, upid, url, name)?;
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
fn bulk_clone(app: ArgMatches) -> Result<(), Box<dyn Error>> {
    let max = app.get_one::<String>("Max").unwrap().parse::<i32>()?;
    let min = app.get_one::<String>("Min").unwrap().parse::<i32>()?;
    let name = app.get_one::<String>("Name").unwrap();
    let src = app.get_one::<String>("Source").unwrap();
    let url = app.get_one::<String>("Url").unwrap();
    let username = app.get_one::<String>("Username").unwrap();
    let password = app.get_one::<String>("Password").unwrap();
    let token = get_token(&mut username.clone(), password, url)?;

    let n_url = format!("{}/api2/json/nodes/{}/qemu/{}/clone", url, name, src);
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
    for i in min..max + 1 {
        let mut json_data = HashMap::new();
        json_data.insert("newid", i.to_string());
        json_data.insert("node", name.clone().to_owned());
        json_data.insert("vmid", src.clone().to_owned());
        let text = client
            .post(&n_url)
            .headers(headers.clone())
            .json(&json_data)
            .send()?
            .text()?;
        let upid: UPIDData = serde_json::de::from_str::<UPIDData>(text.as_str())?;
        finished(headers.clone(), upid, url, name)?;
    }
    Ok(())
}
fn bulk_destroy(app: ArgMatches) -> Result<(), Box<dyn Error>> {
    let max = app.get_one::<String>("Max").unwrap().parse::<i32>()?;
    let min = app.get_one::<String>("Min").unwrap().parse::<i32>()?;
    let client = ClientBuilder::new()
        .danger_accept_invalid_certs(true)
        .build()?;
    let name = app.get_one::<String>("Name").unwrap();
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
    for i in min..max + 1 {
        let n_url = format!("{}/api2/json/nodes/{}/qemu/{}", url, name, i.to_string());
        client
            .delete(n_url)
            .headers(headers.clone())
            .send()?
            .text()?;
        println!("VMID {} destroyed", i);
    }
    Ok(())
}

fn finished(
    headers: HeaderMap,
    upid: UPIDData,
    url: &String,
    name: &String,
) -> Result<(), Box<dyn Error>> {
    let n_url = format!(
        "{}/api2/json/nodes/{}/tasks/{}/status",
        url, name, upid.data
    );
    let client = ClientBuilder::new()
        .danger_accept_invalid_certs(true)
        .build()?;
    loop {
        let resp = client
            .get(n_url.clone())
            .headers(headers.clone())
            .send()?
            .text()?;
        let job_details = match serde_json::de::from_str::<JobData>(resp.as_str()) {
            Ok(jobdata) => jobdata,
            Err(_) => continue,
        };
        if job_details.data.exitstatus == String::from("OK") {
            println!("Finished!");
            break;
        } else {
            println!("{}", job_details.data.exitstatus);
        }
    }

    Ok(())
}
