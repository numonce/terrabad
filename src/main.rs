use clap::{Arg, ArgMatches, Command};
use reqwest::header::{HeaderMap, HeaderValue, COOKIE};
use reqwest::ClientBuilder;
use serde::Deserialize;
use serde_json::Map;
use serde_json::Value;
use std::{collections::HashMap, error::Error, sync::Arc};
use tokio::sync::Semaphore;

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
#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
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
                .value_parser(["clone", "destroy", "bulk_clone", "bulk_destroy"]),
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
        "clone" => create_clone(app).await?,
        "destroy" => destroy_vm(app).await?,
        "bulk_clone" => bulk_clone(app).await?,
        "bulk_destroy" => bulk_destroy(app).await?,
        _ => panic!("asdfasdf"),
    }
    Ok(())
}

async fn get_token(
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
    let text = client
        .post(url)
        .json(&json_data)
        .send()
        .await?
        .text()
        .await?;
    let token: TokenData = serde_json::de::from_str::<TokenData>(&text)?;

    Ok(token)
}

async fn create_clone(app: ArgMatches) -> Result<(), Box<dyn Error>> {
    let name = app.get_one::<String>("Name").unwrap();
    let dst = app.get_one::<String>("Destination").unwrap();
    let src = app.get_one::<String>("Source").unwrap();
    let url = app.get_one::<String>("Url").unwrap();
    let clone_type = app.get_one::<String>("Clone_type").unwrap();
    let username = app.get_one::<String>("Username").unwrap();
    let password = app.get_one::<String>("Password").unwrap();
    let token = get_token(&mut username.clone(), password, url).await?;
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
        .send()
        .await?
        .text()
        .await?;
    let upid: UPIDData = serde_json::de::from_str::<UPIDData>(text.as_str())?;
    finished(headers, upid, url, name).await?;
    Ok(())
}

async fn destroy_vm(app: ArgMatches) -> Result<(), Box<dyn Error>> {
    let client = ClientBuilder::new()
        .danger_accept_invalid_certs(true)
        .build()?;
    let name = app.get_one::<String>("Name").unwrap();
    let src = app.get_one::<String>("Source").unwrap();
    let url = app.get_one::<String>("Url").unwrap();
    let username = app.get_one::<String>("Username").unwrap();
    let password = app.get_one::<String>("Password").unwrap();
    let token = get_token(&mut username.clone(), password, url).await?;
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
        .send()
        .await?
        .text()
        .await?;
    let upid: UPIDData = serde_json::de::from_str::<UPIDData>(text.as_str())?;
    finished(headers, upid, url, name).await?;
    Ok(())
}
async fn bulk_clone(app: ArgMatches) -> Result<(), Box<dyn Error>> {
    let max = app.get_one::<String>("Max").unwrap().parse::<i32>()?;
    let min = app.get_one::<String>("Min").unwrap().parse::<i32>()?;
    let name = app.get_one::<String>("Name").unwrap();
    let src = app.get_one::<String>("Source").unwrap();
    let url = app.get_one::<String>("Url").unwrap();
    let username = app.get_one::<String>("Username").unwrap();
    let password = app.get_one::<String>("Password").unwrap();
    let token = get_token(&mut username.clone(), password, url).await?;
    let full = match app.get_one::<String>("Clone_type").unwrap().as_str() {
        "linked" => false,
        "full" => true,
        _ => false,
    };
    let n_url = format!("{}/api2/json/nodes/{}/qemu/{}/clone", url, name, src);
    let client = reqwest::ClientBuilder::new()
        .danger_accept_invalid_certs(true)
        .build()?;
    let new_cookie = format!("PVEAuthCookie={}", token.data.ticket);
    let mut headers = HeaderMap::new();
    headers.insert(COOKIE, HeaderValue::from_str(new_cookie.as_str())?);
    headers.insert(
        "Csrfpreventiontoken",
        HeaderValue::from_str(token.data.csrf.as_str())?,
    );

    let semaphore = Arc::new(Semaphore::new(2));
    let jobs: Vec<_> = (min..max + 1).collect();
    let tasks: Vec<_> = jobs
        .into_iter()
        .map(|newid| {
            let permit = semaphore.clone();
            let mut json_data = Map::new();
            json_data.insert(String::from("newid"), Value::String(newid.to_string()));
            json_data.insert(String::from("node"), Value::String(name.clone()));
            json_data.insert(String::from("vmid"), Value::String(src.clone()));
            json_data.insert(String::from("full"), Value::Bool(full.clone()));
            let url = url.clone();
            let client = client.clone();
            let src = src.clone();
            let name = name.clone();
            let n_url = n_url.clone();
            let headers = headers.clone();
            tokio::spawn(async move {
                let _permit = permit.acquire().await.unwrap();
                let text = client
                    .clone()
                    .post(n_url)
                    .headers(headers.clone())
                    .json(&json_data.clone())
                    .send()
                    .await
                    .unwrap()
                    .text()
                    .await
                    .unwrap();
                let upid: UPIDData = serde_json::de::from_str::<UPIDData>(text.as_str()).unwrap();
                finished(headers.clone(), upid, &url, &name).await.unwrap();
                println!("VMID {} cloned from {}", newid, src);
            })
        })
        .collect();
    for task in tasks {
        task.await?;
    }

    Ok(())
}
async fn bulk_destroy(app: ArgMatches) -> Result<(), Box<dyn Error>> {
    let max = app.get_one::<String>("Max").unwrap().parse::<i32>()?;
    let min = app.get_one::<String>("Min").unwrap().parse::<i32>()?;
    let client = ClientBuilder::new()
        .danger_accept_invalid_certs(true)
        .build()?;
    let name = app.get_one::<String>("Name").unwrap();
    let url = app.get_one::<String>("Url").unwrap();
    let username = app.get_one::<String>("Username").unwrap();
    let password = app.get_one::<String>("Password").unwrap();
    let token = get_token(&mut username.clone(), password, url).await?;
    let new_cookie = format!("PVEAuthCookie={}", token.data.ticket);
    let mut headers = HeaderMap::new();
    headers.insert(COOKIE, HeaderValue::from_str(new_cookie.as_str())?);
    headers.insert(
        "Csrfpreventiontoken",
        HeaderValue::from_str(token.data.csrf.as_str())?,
    );

    let semaphore = Arc::new(Semaphore::new(2));
    let jobs: Vec<_> = (min..max + 1).collect();
    let tasks: Vec<_> = jobs
        .into_iter()
        .map(|newid| {
            let n_url = format!(
                "{}/api2/json/nodes/{}/qemu/{}",
                url,
                name,
                newid.to_string()
            );
            let url = url.clone();
            let client = client.clone();
            let name = name.clone();
            let n_url = n_url.clone();
            let headers = headers.clone();
            let permit = semaphore.clone();
            tokio::spawn(async move {
                let _permit = permit.acquire().await.unwrap();
                let text = client
                    .delete(n_url)
                    .headers(headers.clone())
                    .send()
                    .await
                    .unwrap()
                    .text()
                    .await
                    .unwrap();
                let upid: UPIDData = serde_json::de::from_str::<UPIDData>(text.as_str()).unwrap();
                finished(headers.clone(), upid, &url, &name).await.unwrap();
                println!("VMID {} destroyed", newid.to_string());
            })
        })
        .collect();
    for task in tasks {
        task.await?;
    }
    Ok(())
}

async fn finished(
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
            .send()
            .await?
            .text()
            .await?;
        let job_details = match serde_json::de::from_str::<JobData>(resp.as_str()) {
            Ok(jobdata) => jobdata,
            Err(_) => continue,
        };
        if job_details.data.exitstatus == String::from("OK") {
            break;
        }
        if job_details.data.exitstatus == String::from("ERROR") {
            println!("VMID {:?} {:?}", upid, job_details.data.exitstatus);
        }
    }

    Ok(())
}
