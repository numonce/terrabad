mod auth;
mod mgmt;
use clap::{Arg, Command};
use std::error::Error;
#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let app =
        Command::new("terrabad")
            .author("numonce")
            .about("A tool for managing proxmox functions written in pure rust.")
            .version("1.0.0")
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
                    .value_parser([
                        "clone",
                        "destroy",
                        "bulk_clone",
                        "bulk_destroy",
                        "bulk_start",
                        "bulk_stop",
                    ]),
            )
            .arg(
                Arg::new("Name").long("name").short('n').help(
                    "Desired name of the created VM. For bulk actions this will add a number.",
                ),
            )
            .arg(
                Arg::new("Node")
                    .long("node")
                    .short('N')
                    .required(true)
                    .help("Name of the node"),
            )
            .arg(
                Arg::new("Source")
                    .long("source")
                    .short('s')
                    .help("Source template VMID for action."),
            )
            .arg(Arg::new("Destination").long("destination").short('d').help(
                "Destination template VMID for action. This is only needed for single actions.",
            ))
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
                    .default_value("1"),
            )
            .get_matches();
    match app.get_one::<String>("Action").unwrap().as_str() {
        "clone" => mgmt::create_clone(app).await?,
        "destroy" => mgmt::destroy_vm(app).await?,
        "bulk_clone" => mgmt::bulk_clone(app).await?,
        "bulk_destroy" => mgmt::bulk_destroy(app).await?,
        "bulk_start" => mgmt::bulk_start(app).await?,
        "bulk_stop" => mgmt::bulk_stop(app).await?,
        _ => panic!("Something incredibly bad occured if you can see this."),
    }
    Ok(())
}
