mod arguments;
mod basemap;
mod chart;
mod domain;
mod dots;
mod geometry;
mod hazard;
mod import_obj;
mod instance;
mod instanced_item;
mod methods;
#[allow(clippy::all)]
mod power_system_capnp;
mod probe;
mod ruler;
mod state;
mod summary;
mod texture;
mod utility;

use state::*;

use arguments::Arguments;
use clap::Parser;
use colabrodo_server::server::*;

use dots::*;

/// Entry point for the noodles_grid application.
///
/// Initializes logging, parses arguments, loads dataset, sets up server state,
/// advertises via mDNS, and runs the main server loop.
#[tokio::main]
async fn main() {
    env_logger::init();

    // Parse command-line arguments
    let args = Arguments::parse();

    // Use specified port or fall back to default (50000)
    let port = args.port.unwrap_or(50000u16);

    let mut opts = ServerOptions::default();

    opts.host.set_port(Some(port)).unwrap();

    println!("Connect clients to port: {}", port);

    // Create a new blank server state
    let state = ServerState::new();

    // Load power system dataset from file
    let data = load_data(&args);
    let data_title = data.title.clone();
    log::info!("Loaded dataset: {data_title}");

    let app_state = GridState::new(state.clone(), data);

    GridState::post_setup(&state, &app_state);

    // Perform initial work to populate instances
    {
        let mut lock = app_state.lock().unwrap();
        recompute_all(&mut lock, &mut state.lock().unwrap());
    }

    // Start mDNS service to advertise server on local network
    let mdns = mdns_publish(opts.host.port().unwrap(), data_title);

    // Enter server main loop (awaits incoming client connections)
    server_main(opts, state).await;

    // Gracefully shut down mDNS service on exit
    mdns.shutdown().unwrap();
}

/// Loads the power system dataset from the specified arguments.
///
/// Panics if loading fails.
fn load_data(args: &Arguments) -> PowerSystem {
    load_powersystem(&args.pack_path).expect("loading powersystem")
}

/// Publishes the server via mDNS/Bonjour for easy local discovery.
///
/// Registers the service under `_noodles._tcp.local.` with hostname and IP addresses.
fn mdns_publish(port: u16, name: String) -> mdns_sd::ServiceDaemon {
    let mdns = mdns_sd::ServiceDaemon::new().expect("unable to create mdns daemon");

    const SERVICE_TYPE: &str = "_noodles._tcp.local.";

    let instance_name = format!("grid: {name}");

    // Gather all non-loopback IPv4 addresses on the machine
    if let Ok(nif) = local_ip_address::list_afinet_netifas() {
        let ip_list: Vec<_> = nif
            .iter()
            .filter_map(|f| match f.1 {
                std::net::IpAddr::V4(ipv4_addr) => Some(ipv4_addr),
                _ => None,
            })
            .filter_map(|f| {
                let str = f.to_string();
                if str != "127.0.0.1" {
                    Some(str)
                } else {
                    None
                }
            })
            .collect();

        let hname = hostname::get().unwrap_or_else(|_| ip_list.first().unwrap().into());

        let hname = hname.into_string().unwrap();

        let host = format!("{}.local.", hname);

        // Construct mDNS service record with IPs and port
        let srv_info = mdns_sd::ServiceInfo::new(
            SERVICE_TYPE,
            &instance_name,
            &host,
            ip_list.as_slice(),
            port,
            None,
        )
        .expect("unable to build MDNS service information");

        log::info!("registering MDNS SD on {name} {ip_list:?}");

        // Register service and log if it fails
        if mdns.register(srv_info).is_err() {
            log::warn!("unable to register MDNS!");
        }
    }

    mdns
}
