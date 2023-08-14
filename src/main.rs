use clap::{Arg, ArgMatches, Command};
use reqwest::header::{HeaderMap, HeaderValue, COOKIE};
use reqwest::ClientBuilder;
use serde::Deserialize;
use serde_json::Map;
use serde_json::Value;
use std::str::FromStr;
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
    let nodename = app.get_one::<String>("Node").unwrap();
    let dst = app.get_one::<String>("Destination").unwrap();
    let src = app.get_one::<String>("Source");
    let url = app.get_one::<String>("Url").unwrap();
    let clone_type = app.get_one::<String>("Clone_type").unwrap();
    let username = app.get_one::<String>("Username").unwrap();
    let password = app.get_one::<String>("Password").unwrap();
    let token = get_token(&mut username.clone(), password, url).await?;
    let name = app.get_one::<String>("Name");

    let name = match name {
        Some(n) => n,
        None => "",
    };

    let src = match src {
        Some(e) => e,
        None => panic!("The argument requires a source VMID"),
    };
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
    if name != "" {
        json_data.insert("name".to_string(), Value::String(name.to_owned()));
    }
    json_data.insert("newid".to_string(), Value::String(dst.to_owned()));
    json_data.insert("node".to_string(), Value::String(nodename.to_owned()));
    json_data.insert(
        "vmid".to_string(),
        serde_json::Value::String(src.to_owned()),
    );
    json_data.insert("full".to_string(), Value::Bool(full));
    let n_url = format!("{}/api2/json/nodes/{}/qemu/{}/clone", url, nodename, src);
    let text = client
        .post(n_url)
        .headers(headers.clone())
        .json(&json_data)
        .send()
        .await?
        .text()
        .await?;
    let upid: UPIDData = serde_json::de::from_str::<UPIDData>(text.as_str())?;
    finished(headers, upid, url, nodename).await?;
    Ok(())
}

async fn destroy_vm(app: ArgMatches) -> Result<(), Box<dyn Error>> {
    let client = ClientBuilder::new()
        .danger_accept_invalid_certs(true)
        .build()?;
    let name = app.get_one::<String>("Node").unwrap();
    let src = app.get_one::<String>("Source");
    let url = app.get_one::<String>("Url").unwrap();
    let username = app.get_one::<String>("Username").unwrap();
    let password = app.get_one::<String>("Password").unwrap();
    let token = get_token(&mut username.clone(), password, url).await?;
    let new_cookie = format!("PVEAuthCookie={}", token.data.ticket);
    let src = match src {
        Some(e) => e,
        None => panic!("The argument requires a source VMID"),
    };
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
    let nodename = app.get_one::<String>("Node").unwrap();
    let src = app.get_one::<String>("Source");
    let url = app.get_one::<String>("Url").unwrap();
    let username = app.get_one::<String>("Username").unwrap();
    let password = app.get_one::<String>("Password").unwrap();
    let token = get_token(&mut username.clone(), password, url).await?;
    let name = app.get_one::<String>("Name");

    let name = match name {
        Some(n) => n,
        None => "",
    };
    let src = match src {
        Some(e) => e,
        None => panic!("The argument requires a source VMID"),
    };
    let full = match app.get_one::<String>("Clone_type").unwrap().as_str() {
        "linked" => false,
        "full" => true,
        _ => false,
    };
    let n_url = format!("{}/api2/json/nodes/{}/qemu/{}/clone", url, nodename, src);
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

    let semaphore = Arc::new(Semaphore::new(
        app.get_one::<String>("Threads").unwrap().parse::<usize>()?,
    ));
    let jobs: Vec<_> = (min..max + 1).collect();
    let tasks: Vec<_> = jobs
        .into_iter()
        .map(|newid| {
            let permit = semaphore.clone();
            let mut json_data = Map::new();
            if name != "" {
                let temp_name = format!("{}{}", name, (newid - min));
                json_data.insert(String::from("name"), Value::String(temp_name));
            }
            json_data.insert(String::from("newid"), Value::String(newid.to_string()));
            json_data.insert(String::from("node"), Value::String(nodename.clone()));
            json_data.insert(String::from("vmid"), Value::String(src.clone()));
            json_data.insert(String::from("full"), Value::Bool(full.clone()));
            let url = url.clone();
            let client = client.clone();
            let src = src.clone();
            let name = nodename.clone();
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
                let upid = match serde_json::de::from_str::<UPIDData>(text.as_str()) {
                    Ok(upid) => upid,
                    Err(e) => panic!("Expected upid, got {:?}", e),
                };
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
    let name = app.get_one::<String>("Node").unwrap();
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

    let semaphore = Arc::new(Semaphore::new(
        app.get_one::<String>("Threads").unwrap().parse::<usize>()?,
    ));
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
                let request = match client.delete(&n_url).headers(headers.clone()).send().await {
                    Ok(c) => c,
                    Err(_) => panic!("Encountered an error. Does the VMID exist?"),
                };
                if request.status() == reqwest::StatusCode::from_str("200").unwrap() {
                    let text = request.text().await.unwrap(); // Same with the aformetioned comment
                    let upid = serde_json::de::from_str::<UPIDData>(text.as_str()).unwrap();
                    finished(headers.clone(), upid, &url, &name).await.unwrap();
                    println!("{} destroyed", newid);
                } else {
                    println!(
                        "Server returned: {}. Make sure {} is valid.",
                        request.status(),
                        n_url
                    );
                }
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
        } else {
            println!("{:?}", job_details.data.exitstatus);
        }
    }

    Ok(())
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
                .long("action")
                .short('a')
                .help("clone, etc...")
                .required(true)
                .value_parser(["clone", "destroy", "bulk_clone", "bulk_destroy"]),
        )
        .arg(
            Arg::new("Name")
                .long("name")
                .short('n')
                .help("Desired name of the created VM. For bulk actions this will add a number."),
        )
        .arg(
            Arg::new("Node")
                .long("node")
                .short('N')
                .required(true)
                .help("Node of the node"),
        )
        .arg(
            Arg::new("Source")
                .long("source")
                .short('s')
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
                .short('T')
                .help("Type of clone. Can either be linked or full.")
                .default_value("linked")
                .value_parser(["linked", "full"]),
        )
        .arg(
            Arg::new("Threads")
                .long("threads")
                .short('t')
                .help("Number of workers.")
                .default_value("3"),
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
