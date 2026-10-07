use super::{LOOPBACK_HOST, PORT_BASE};

use futures::StreamExt;
use log::{error, warn};
use tokio::net::{TcpListener, TcpStream};

/// Start the WebSocket server that plugins communicate with.
pub(super) async fn init_websocket_server() {
	let listener = match TcpListener::bind((LOOPBACK_HOST, *PORT_BASE)).await {
		Ok(listener) => listener,
		Err(error) => {
			error!("Failed to bind plugin WebSocket server to socket: {}", error);
			return;
		}
	};

	#[cfg(windows)]
	{
		use std::os::windows::io::AsRawSocket;
		use windows_sys::Win32::Foundation::{HANDLE_FLAG_INHERIT, SetHandleInformation};

		unsafe { SetHandleInformation(listener.as_raw_socket() as _, HANDLE_FLAG_INHERIT, 0) };
	}

	while let Ok((stream, _)) = listener.accept().await {
		accept_connection(stream).await;
	}
}

/// Handle incoming data from a WebSocket connection.
async fn accept_connection(stream: TcpStream) {
	let mut socket = match tokio_tungstenite::accept_async(stream).await {
		Ok(socket) => socket,
		Err(error) => {
			warn!("Failed to complete WebSocket handshake: {}", error);
			return;
		}
	};

	// A plugin that connects and drops again must not take the accept loop with it.
	let Some(Ok(register_event)) = socket.next().await else {
		return;
	};
	let Ok(payload) = register_event.clone().into_text() else {
		warn!("Ignoring a plugin connection that opened with a non-text message");
		return;
	};
	match serde_json::from_str(&payload) {
		Ok(event) => crate::events::register_plugin(event, socket).await,
		Err(_) => {
			let _ = crate::events::inbound::process_incoming_message(Ok(register_event), "", false).await;
		}
	}
}
