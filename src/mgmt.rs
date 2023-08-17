use clap::ArgMatches;
use reqwest::header::HeaderMap;
use reqwest::ClientBuilder;
use serde::Deserialize;
use serde_json::Map;
use serde_json::Value;
use std::{error::Error, sync::Arc};
use tokio::sync::Semaphore;

//This struct is simply used to handle instances when the api returns "Data":null.
#[derive(Deserialize)]
pub struct NULLData {
    data: Option<String>,
}
//This struct is to handle the upid, which is the unique identifier proxmox returns when you
//submit a job.
#[derive(Deserialize, Debug)]
pub struct UPIDData {
    data: String,
}
//The next two structs handle the query of a job via the aformetioned upid. Since it returns a key
//with key pairs we have to build a struct the feeds into a struct.
#[derive(Deserialize, Debug)]
pub struct JobData {
    pub data: Job,
}

#[derive(Deserialize, Debug)]
pub struct Job {
    pub exitstatus: String,
}
//This functions creates single clones.
pub async fn create_clone(app: ArgMatches) -> Result<(), Box<dyn Error>> {
    let nodename = app.get_one::<String>("Node").unwrap();
    let dst = app.get_one::<String>("Destination").unwrap();
    let src = app.get_one::<String>("Source");
    let mut url = app.get_one::<String>("Url").unwrap().to_owned(); //Handles the format of https://proxmox/ vs https://proxmox
    if url.ends_with('/') {
        url.pop();
    }
    let clone_type = app.get_one::<String>("Clone_type").unwrap();
    let username = app.get_one::<String>("Username").unwrap();
    let password = app.get_one::<String>("Password").unwrap();
    //Grabs a headermap with the pvecookie and csrfprevention token.
    let token = super::auth::get_token(&mut username.clone(), password, &url).await?;
    let name = app.get_one::<String>("Name");

    let name = match name {
        //This handles the optional parameter of naming the new cloned vm.
        Some(n) => n,
        None => "",
    };

    let src = match src {
        //This is a way to make this parameter a requirement for this function,
        //but not the whole program.
        Some(e) => e,
        None => panic!("The argument requires a source VMID"),
    };
    let full = match clone_type.as_str() {
        "linked" => false,
        "full" => true,
        _ => false,
    };
    let client = ClientBuilder::new()
        .danger_accept_invalid_certs(true) //Allows us to ignore the invalid ssl cert
        .build()?;
    //Using the Map and Value structs from serde_json allows us to have a hashmap with mixed data types.
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
    //The below code sends between 1-2 requests. It doesn't actually implement a way to determine
    //if the src template is lxc or qemu.
    let lxc_url = format!("{}/api2/json/nodes/{}/lxc/{}/clone", url, nodename, src);
    let qemu_url = format!("{}/api2/json/nodes/{}/qemu/{}/clone", url, nodename, src);
    let qemu_response = client
        .post(qemu_url)
        .headers(token.clone())
        .json(&json_data)
        .send()
        .await?;
    if qemu_response.status() == 200 {
        //Takes the upid returned by the submitted job and sends it to a function that returns when
        //the job is finished or errs.
        let upid: UPIDData =
            serde_json::de::from_str::<UPIDData>(qemu_response.text().await?.as_str())?;
        finished(token, upid, &url, nodename).await?;
    } else {
        //LCXs can only be single threaded and full cloned at the moment. So pop the clone type
        //here and replace it with a full clone. Doesn't implement a check for the number of
        //threads tho.
        json_data.remove("full");
        json_data.insert("full".to_string(), Value::Bool(true));
        let lxc_response = client
            .post(lxc_url)
            .headers(token.clone())
            .json(&json_data)
            .send()
            .await?;
        if lxc_response.status() != 200 {
            println!("Unable to clone target. Check arguments and permissions.")
        } else {
            let upid: UPIDData =
                serde_json::de::from_str::<UPIDData>(lxc_response.text().await?.as_str())?;
            finished(token, upid, &url, nodename).await?;
            println!("VMID {} cloned from {}", dst, src);
        }
    }
    Ok(())
}
//This function does much of the same thing as the last one, sends a delete and doesn't send json.
pub async fn destroy_vm(app: ArgMatches) -> Result<(), Box<dyn Error>> {
    let client = ClientBuilder::new()
        .danger_accept_invalid_certs(true)
        .build()?;
    let nodename = app.get_one::<String>("Node").unwrap();

    let src = app.get_one::<String>("Source");
    let mut url = app.get_one::<String>("Url").unwrap().to_owned();
    if url.ends_with('/') {
        url.pop();
    }
    let username = app.get_one::<String>("Username").unwrap();
    let password = app.get_one::<String>("Password").unwrap();
    let token = super::auth::get_token(&mut username.clone(), password, &url).await?;
    let src = match src {
        Some(e) => e,
        None => panic!("The argument requires a source VMID"),
    };
    let qemu_url = format!("{}/api2/json/nodes/{}/qemu/{}", url, nodename, src);
    let lxc_url = format!("{}/api2/json/nodes/{}/lxc/{}", url, nodename, src);
    let qemu_response = client
        .delete(qemu_url)
        .headers(token.clone())
        .send()
        .await?;
    if qemu_response.status() == 200 {
        let upid: UPIDData =
            serde_json::de::from_str::<UPIDData>(qemu_response.text().await?.as_str())?;
        finished(token, upid, &url, nodename).await?;
    } else {
        let lxc_response = client.delete(lxc_url).headers(token.clone()).send().await?;
        if lxc_response.status() != 200 {
            println!("Unable to destroy target. Check arguments and permissions.")
        } else {
            let upid: UPIDData =
                serde_json::de::from_str::<UPIDData>(lxc_response.text().await?.as_str())?;
            finished(token, upid, &url, nodename).await?;
            println!("{} destroyed.", src);
        }
    }
    Ok(())
}
//This does much of the same stuff as create_clone, but uses tokio to thread and send requests
//async.
pub async fn bulk_clone(app: ArgMatches) -> Result<(), Box<dyn Error>> {
    let max = app.get_one::<String>("Max").unwrap().parse::<i32>()?;
    let min = app.get_one::<String>("Min").unwrap().parse::<i32>()?;
    let nodename = app.get_one::<String>("Node").unwrap();
    let src = app.get_one::<String>("Source");
    let mut url = app.get_one::<String>("Url").unwrap().to_owned();
    if url.ends_with('/') {
        url.pop();
    }
    let username = app.get_one::<String>("Username").unwrap();
    let password = app.get_one::<String>("Password").unwrap();
    let token = super::auth::get_token(&mut username.clone(), password, &url).await?;
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
    let qemu_url = format!("{}/api2/json/nodes/{}/qemu/{}/clone", url, nodename, src);
    let lxc_url = format!("{}/api2/json/nodes/{}/lxc/{}/clone", url, nodename, src);
    let client = reqwest::ClientBuilder::new()
        .danger_accept_invalid_certs(true)
        .build()?;
    //Creates a semaphore to control the amount of concurrent jobs running.
    let semaphore = Arc::new(Semaphore::new(
        app.get_one::<String>("Threads").unwrap().parse::<usize>()?,
    ));
    let jobs: Vec<_> = (min..max + 1).collect(); // Creates a vec of the specified range. Inclusive.
                                                 //Creates a vec of the jobs needed to be accomplished.
    let tasks: Vec<_> = jobs
        .into_iter()
        .map(|newid| {
            let permit = semaphore.clone();
            let mut json_data = Map::new();
            let mut temp_name = String::new();
            if name != "" {
                temp_name = format!("{}{}", name, (newid - min));
            }
            json_data.insert(String::from("newid"), Value::String(newid.to_string()));
            json_data.insert(String::from("node"), Value::String(nodename.clone()));
            json_data.insert(String::from("vmid"), Value::String(src.clone()));
            json_data.insert(String::from("full"), Value::Bool(full.clone()));
            let url = url.clone();
            let client = client.clone();
            let src = src.clone();
            let temp_name = temp_name.clone();
            let nodename = nodename.clone();
            let qemu_url = qemu_url.clone();
            let lxc_url = lxc_url.clone();
            let token = token.clone();
            tokio::spawn(async move {
                let _permit = permit.acquire().await.unwrap();
                if temp_name != "" {
                    json_data.insert("name".to_string(), Value::String(temp_name.to_string()));
                }
                let qemu_response = client
                    .clone()
                    .post(qemu_url)
                    .headers(token.clone())
                    .json(&json_data.clone())
                    .send()
                    .await
                    .unwrap();

                if qemu_response.status() == 200 {
                    let upid: UPIDData = serde_json::de::from_str::<UPIDData>(
                        qemu_response.text().await.unwrap().as_str(),
                    )
                    .unwrap();
                    finished(token, upid, &url, &nodename).await.unwrap();
                    println!("VMID {} cloned from {}", newid, src);
                } else {
                    json_data.remove("full");
                    json_data.remove("name");
                    json_data.insert("full".to_string(), Value::Bool(true));
                    if temp_name != "" {
                        json_data
                            .insert("hostname".to_string(), Value::String(temp_name.to_string()));
                    }
                    let lxc_response = client
                        .post(lxc_url)
                        .headers(token.clone())
                        .json(&json_data)
                        .send()
                        .await
                        .unwrap();
                    if lxc_response.status() != 200 {
                        println!("Unable to clone target. Check arguments and permissions.")
                    } else {
                        let upid: UPIDData = match serde_json::de::from_str::<UPIDData>(
                            lxc_response.text().await.unwrap().as_str(),
                        ) {
                            Ok(u) => u,
                            Err(e) => panic!("Program paniced because of {}", e),
                        };
                        finished(token, upid, &url, &nodename).await.unwrap();
                        println!("VMID {} cloned from {}", newid, src);
                    }
                }
            })
        })
        .collect();
    for task in tasks {
        task.await?;
    }
    Ok(())
}
//Does much of the same as the aformetioned function, but deletes instead.
pub async fn bulk_destroy(app: ArgMatches) -> Result<(), Box<dyn Error>> {
    let max = app.get_one::<String>("Max").unwrap().parse::<i32>()?;
    let min = app.get_one::<String>("Min").unwrap().parse::<i32>()?;
    let client = ClientBuilder::new()
        .danger_accept_invalid_certs(true)
        .build()?;
    let name = app.get_one::<String>("Node").unwrap();
    let mut url = app.get_one::<String>("Url").unwrap().to_owned();
    if url.ends_with('/') {
        url.pop();
    }
    let username = app.get_one::<String>("Username").unwrap();
    let password = app.get_one::<String>("Password").unwrap();
    let token = super::auth::get_token(&mut username.clone(), password, &url).await?;
    let semaphore = Arc::new(Semaphore::new(
        app.get_one::<String>("Threads").unwrap().parse::<usize>()?,
    ));
    let jobs: Vec<_> = (min..max + 1).collect();
    let tasks: Vec<_> = jobs
        .into_iter()
        .map(|newid| {
            let qemu_url = format!(
                "{}/api2/json/nodes/{}/qemu/{}",
                url,
                name,
                newid.to_string()
            );
            let lxc_url = format!("{}/api2/json/nodes/{}/lxc/{}", url, name, newid.to_string());
            let url = url.clone();
            let client = client.clone();
            let name = name.clone();
            let qemu_url = qemu_url.clone();
            let lxc_url = lxc_url.clone();
            let token = token.clone();
            let permit = semaphore.clone();
            tokio::spawn(async move {
                let _permit = permit.acquire().await.unwrap();
                let qemu_request =
                    match client.delete(&qemu_url).headers(token.clone()).send().await {
                        Ok(c) => c,
                        Err(_) => panic!("Encountered an error. Does the VMID exist?"),
                    };
                if qemu_request.status() == 200 {
                    let text = qemu_request.text().await.unwrap(); // Same with the aformetioned comment
                    let upid = serde_json::de::from_str::<UPIDData>(text.as_str()).unwrap();
                    finished(token.clone(), upid, &url, &name).await.unwrap();
                    println!("{} destroyed", newid);
                } else {
                    let lxc_request =
                        match client.delete(&lxc_url).headers(token.clone()).send().await {
                            Ok(c) => c,
                            Err(_) => panic!("Encountered an error. Does the VMID exist?"),
                        };
                    if lxc_request.status() == 200 {
                        let text = lxc_request.text().await.unwrap(); // Same with the aformetioned comment
                        let upid = serde_json::de::from_str::<UPIDData>(text.as_str()).unwrap();
                        finished(token.clone(), upid, &url, &name).await.unwrap();
                        println!("{} destroyed", newid);
                    } else {
                        println!(
                            "An error occured in starting {}\nMake sure vmid exists",
                            newid
                        );
                    }
                }
            })
        })
        .collect();
    for task in tasks {
        task.await?;
    }
    Ok(())
}

pub async fn bulk_stop(app: ArgMatches) -> Result<(), Box<dyn Error>> {
    let max = app.get_one::<String>("Max").unwrap().parse::<i32>()?;
    let min = app.get_one::<String>("Min").unwrap().parse::<i32>()?;
    let client = ClientBuilder::new()
        .danger_accept_invalid_certs(true)
        .build()?;
    let name = app.get_one::<String>("Node").unwrap();
    let mut url = app.get_one::<String>("Url").unwrap().to_owned();
    if url.ends_with('/') {
        url.pop();
    }
    let semaphore = Arc::new(Semaphore::new(
        app.get_one::<String>("Threads").unwrap().parse::<usize>()?,
    ));
    let username = app.get_one::<String>("Username").unwrap();
    let password = app.get_one::<String>("Password").unwrap();
    let token = super::auth::get_token(&mut username.clone(), password, &url).await?;
    let jobs: Vec<_> = (min..max + 1).collect();
    let tasks: Vec<_> = jobs
        .into_iter()
        .map(|newid| {
            let qemu_url = format!(
                "{}/api2/json/nodes/{}/qemu/{}/status/stop",
                url,
                name,
                newid.to_string()
            );
            //Starting and stopping things returns a upid and a 200 regardless if the vmid supplied
            //is actually the correct template type to start/stop. So we make a test url to query
            //with the vmid to determine the type and then send the request based on that.
            let checker_url = format!("{}/api2/json/nodes/{}/lxc/{}", url, name, newid.to_string());
            let lxc_url = format!(
                "{}/api2/json/nodes/{}/lxc/{}/status/stop",
                url,
                name,
                newid.to_string()
            );
            let url = url.clone();
            let client = client.clone();
            let name = name.clone();
            let checker_url = checker_url.clone();
            let qemu_url = qemu_url.clone();
            let lxc_url = lxc_url.clone();
            let token = token.clone();
            let permit = semaphore.clone();
            tokio::spawn(async move {
                let _permit = permit.acquire().await.unwrap();
                //check the type of vm
                let checker = client
                    .get(checker_url)
                    .headers(token.clone())
                    .send()
                    .await
                    .unwrap()
                    .text()
                    .await
                    .unwrap();
                match serde_json::de::from_str::<NULLData>(checker.as_str()) {
                    Ok(_) => {
                        //If the response can correctly serialize as "Data":Null then we assume
                        //it's qemu.
                        let qemu_request =
                            match client.post(&qemu_url).headers(token.clone()).send().await {
                                Ok(c) => c,
                                Err(_) => panic!("Encountered an error. Does the VMID exist?"),
                            };
                        if qemu_request.status() == 200 {
                            let text = qemu_request.text().await.unwrap(); // Same with the aformetioned comment
                            let upid = serde_json::de::from_str::<UPIDData>(text.as_str()).unwrap();
                            finished(token.clone(), upid, &url, &name).await.unwrap();
                            println!("{} stopped", newid);
                        } else {
                            println!("Error stopping VMID {}. Does the VM exist?", newid);
                        }
                    }
                    //If the response actually contains data then we assume it's lxc.
                    Err(_) => {
                        let lxc_request =
                            match client.post(&lxc_url).headers(token.clone()).send().await {
                                Ok(c) => c,
                                Err(_) => panic!("Encountered an error. Does the VMID exist?"),
                            };
                        if lxc_request.status() == 200 {
                            let text = lxc_request.text().await.unwrap(); // Same with the aformetioned comment
                            let upid = serde_json::de::from_str::<UPIDData>(text.as_str()).unwrap();
                            finished(token.clone(), upid, &url, &name).await.unwrap();
                            println!("{} stopped", newid);
                        } else {
                            println!("Error stopping VMID {}. Does the VMID exist?", newid);
                        }
                    }
                };
            })
        })
        .collect();
    for task in tasks {
        task.await?;
    }
    Ok(())
}
pub async fn bulk_start(app: ArgMatches) -> Result<(), Box<dyn Error>> {
    let max = app.get_one::<String>("Max").unwrap().parse::<i32>()?;
    let min = app.get_one::<String>("Min").unwrap().parse::<i32>()?;
    let client = ClientBuilder::new()
        .danger_accept_invalid_certs(true)
        .build()?;
    let name = app.get_one::<String>("Node").unwrap();
    let mut url = app.get_one::<String>("Url").unwrap().to_owned();
    if url.ends_with('/') {
        url.pop();
    }
    let semaphore = Arc::new(Semaphore::new(
        app.get_one::<String>("Threads").unwrap().parse::<usize>()?,
    ));
    let username = app.get_one::<String>("Username").unwrap();
    let password = app.get_one::<String>("Password").unwrap();
    let token = super::auth::get_token(&mut username.clone(), password, &url).await?;
    let jobs: Vec<_> = (min..max + 1).collect();
    let tasks: Vec<_> = jobs
        .into_iter()
        .map(|newid| {
            let qemu_url = format!(
                "{}/api2/json/nodes/{}/qemu/{}/status/start",
                url,
                name,
                newid.to_string()
            );

            let checker_url = format!("{}/api2/json/nodes/{}/lxc/{}", url, name, newid.to_string());
            let lxc_url = format!(
                "{}/api2/json/nodes/{}/lxc/{}/status/start",
                url,
                name,
                newid.to_string()
            );
            let url = url.clone();
            let client = client.clone();
            let name = name.clone();
            let checker_url = checker_url.clone();
            let qemu_url = qemu_url.clone();
            let lxc_url = lxc_url.clone();
            let token = token.clone();
            let permit = semaphore.clone();
            tokio::spawn(async move {
                let _permit = permit.acquire().await.unwrap();
                //check the type of vm
                let checker = client
                    .get(checker_url)
                    .headers(token.clone())
                    .send()
                    .await
                    .unwrap()
                    .text()
                    .await
                    .unwrap();
                match serde_json::de::from_str::<NULLData>(checker.as_str()) {
                    Ok(_) => {
                        let qemu_request =
                            match client.post(&qemu_url).headers(token.clone()).send().await {
                                Ok(c) => c,
                                Err(_) => panic!("Encountered an error. Does the VMID exist?"),
                            };
                        if qemu_request.status() == 200 {
                            let text = qemu_request.text().await.unwrap(); // Same with the aformetioned comment
                            let upid = serde_json::de::from_str::<UPIDData>(text.as_str()).unwrap();
                            finished(token.clone(), upid, &url, &name).await.unwrap();
                            println!("{} started", newid);
                        }
                    }
                    Err(_) => {
                        let lxc_request =
                            match client.post(&lxc_url).headers(token.clone()).send().await {
                                Ok(c) => c,
                                Err(_) => panic!("Encountered an error. Does the VMID exist?"),
                            };
                        if lxc_request.status() == 200 {
                            let text = lxc_request.text().await.unwrap(); // Same with the aformetioned comment
                            let upid = serde_json::de::from_str::<UPIDData>(text.as_str()).unwrap();
                            finished(token.clone(), upid, &url, &name).await.unwrap();
                            println!("{} started", newid);
                        } else {
                            println!("Error starting VMID {}. Does the LXC exist?", newid);
                        }
                    }
                };
            })
        })
        .collect();
    for task in tasks {
        task.await?;
    }
    Ok(())
}

pub async fn finished(
    headers: HeaderMap,
    upid: UPIDData,
    url: &String,
    name: &String,
) -> Result<(), Box<dyn Error>> {
    tokio::time::sleep(tokio::time::Duration::from_millis(350)).await;
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
            break;
        }
    }

    Ok(())
}
