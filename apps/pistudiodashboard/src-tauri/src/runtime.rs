//! The client runtime and the engine hooks that reach it: requests leave through
//! `send_to_daemon` and `daemon_link`; events come back through the feed pump.

use std::sync::Arc;
use std::sync::mpsc::Sender;

use pistudiodashboard_client::{ClientConfig, ClientHandle, Secret as ClientSecret, SendError as ClientSendError};
use pistudiodashboard_engine::{EngineServices, LinkCommand, SendError};
use pistudiodashboard_shell::Envelope;

use crate::feed_pump;

/// Start the client (its own 2-worker tokio runtime) and the feed pump into `requests`.
pub fn start_client(requests: Sender<Envelope>) -> Arc<ClientHandle> {
    let (client, events) = ClientHandle::start(ClientConfig::new(env!("CARGO_PKG_VERSION")));
    if feed_pump::spawn_ticker(requests.clone()).is_none() {
        log::error!("the usage tick thread could not start: usage will not refresh");
    }
    if feed_pump::spawn(events, requests).is_none() {
        log::error!("the feed pump thread could not start: daemon events will not reach the app");
    }
    Arc::new(client)
}

fn map_send_error(e: ClientSendError) -> SendError {
    match e {
        ClientSendError::NotConnected => SendError::NotConnected,
        ClientSendError::Busy | ClientSendError::QueueFull => SendError::QueueFull,
    }
}

/// Fill the connection hooks of `services`. The hooks accept only `OutboundRequest`, so no raw
/// frame can be sent.
pub fn install_hooks(services: &mut EngineServices, client: &Arc<ClientHandle>) {
    let sender = Arc::clone(client);
    // `serverId` addresses the live connection of that daemon; the client resolves it.
    services.send_to_daemon = Some(Box::new(move |server_id, request| sender.request(server_id, request, "").map_err(map_send_error)));
    let link = Arc::clone(client);
    services.daemon_link = Some(Box::new(move |entry_id, command| match command {
        LinkCommand::Connect { entry, password } => link.connect(entry, password.map(|p| ClientSecret::new(p.expose()))).map_err(|e| e.to_string()),
        LinkCommand::Restart { entry, password } => link.restart(entry_id, entry, password.map(|p| ClientSecret::new(p.expose()))).map_err(|e| e.to_string()),
        LinkCommand::Disconnect => {
            link.disconnect(entry_id);
            Ok(())
        }
        LinkCommand::Remove => {
            link.remove(entry_id);
            Ok(())
        }
    }));
}
