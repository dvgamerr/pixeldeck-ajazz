use std::path::{Path, PathBuf};

use tiny_http::{Header, Request, Response, Server};

const PROPERTY_INSPECTOR_DAISYUI: &str = include_str!(concat!(env!("OUT_DIR"), "/property_inspector_daisyui.css"));
const PROPERTY_INSPECTOR_DAISYUI_PATH: &str = "/__opendeck/daisyui.css";
const PROPERTY_INSPECTOR_THEME: &str = include_str!("property_inspector_theme.css");

fn inject_property_inspector_theme(content: &mut String) {
	let theme = format!(
		r#"<link id="opendeck-property-inspector-daisyui" rel="stylesheet" href="{PROPERTY_INSPECTOR_DAISYUI_PATH}?v={}"><style id="opendeck-property-inspector-theme">{PROPERTY_INSPECTOR_THEME}</style>"#,
		env!("OPENDECK_DAISYUI_HASH")
	);
	let lowercase_content = content.to_ascii_lowercase();
	if let Some(head_end) = lowercase_content.rfind("</head>") {
		content.insert_str(head_end, &theme);
	} else {
		content.push_str(&theme);
	}
}

fn mime(extension: &str) -> String {
	match extension {
		"htm" | "html" | "xhtml" => "text/html".to_owned(),
		"js" | "cjs" | "mjs" => "text/javascript".to_owned(),
		"css" => "text/css".to_owned(),
		"png" | "jpeg" | "gif" | "webp" => format!("image/{}", extension),
		"jpg" => "image/jpeg".to_owned(),
		"svg" => "image/svg+xml".to_owned(),
		_ => "application/octet-stream".to_owned(),
	}
}

const PROPERTY_INSPECTOR_SCRIPT: &str = r#"
				<div id="opendeck_iframe_container" style="position: absolute; z-index: 100; top: 0; left: 0; width: 100%; height: 100%; display: none;"></div>
				<script>
					const opendeck_window_open = window.open;
					const opendeck_iframe_container = document.getElementById("opendeck_iframe_container");

					window.addEventListener("message", ({ data }) => {
						if (data.event == "connect") {
							event.stopImmediatePropagation();
							const theme = data.theme ?? "dark";
							document.documentElement.dataset.theme = theme;
							document.documentElement.dataset.opendeckTheme = theme;
							if (typeof connectOpenActionSocket === "function") connectOpenActionSocket(...data.payload);
							else connectElgatoStreamDeckSocket(...data.payload);
						} else if (data.event == "theme") {
							const theme = data.theme ?? "dark";
							document.documentElement.dataset.theme = theme;
							document.documentElement.dataset.opendeckTheme = theme;
						} else if (data.event == "windowClosed") {
							event.stopImmediatePropagation();
							if (opendeck_iframe_container.firstElementChild) opendeck_iframe_container.firstElementChild.remove();
							opendeck_iframe_container.style.display = "none";
						}
					});

					window.open = (url, target) => {
						if (target && !(target == "_self" || target == "_top")) {
							top.postMessage({ event: "openUrl", payload: url.startsWith("http") ? url : new URL(url, window.location.href).href }, "*");
							return;
						}
						let iframe = document.createElement("iframe");
						iframe.style.flexGrow = "1";
						iframe.onload = () => {
							iframe.contentWindow.opener = window;
							iframe.contentWindow.onbeforeunload = () => top.postMessage({ event: "windowClosed", payload: window.name }, "*");
							iframe.contentWindow.close = () => { iframe.contentWindow.onbeforeunload(); iframe.remove(); };
							iframe.contentWindow.document.body.style.overflowY = "auto";
							const theme = document.documentElement.dataset.theme ?? "dark";
							iframe.contentWindow.document.documentElement.dataset.theme = theme;
							iframe.contentWindow.document.documentElement.dataset.opendeckTheme = theme;
						};
						iframe.src = url.startsWith("http") ? url : url + "|opendeck_property_inspector_child";
						if (opendeck_iframe_container.firstElementChild) opendeck_iframe_container.firstElementChild.remove();
						opendeck_iframe_container.appendChild(iframe);
						opendeck_iframe_container.style.display = "flex";
						top.postMessage({ event: "windowOpened", payload: window.name }, "*");
						return iframe.contentWindow;
					};

					const opendeck_window_fetch = window.fetch;
					let opendeck_fetch_count = 0;
					let opendeck_fetch_promises = {};
					window.addEventListener("message", ({ data }) => {
						if (data.event == "fetchResponse") {
							event.stopImmediatePropagation();
							const response = new Response(data.payload.response.body, data.payload.response);
							Object.defineProperty(response, "url", { value: data.payload.response.url });
							opendeck_fetch_promises[data.payload.id].resolve(response);
							delete opendeck_fetch_promises[data.payload.id];
						} else if (data.event == "fetchError") {
							event.stopImmediatePropagation();
							opendeck_fetch_promises[data.payload.id].reject(data.payload.error);
							delete opendeck_fetch_promises[data.payload.id];
						}
					});
					window.fetch = (...args) => {
						if (args.length) args[0] = new URL(args[0], window.location.href).href;
						top.postMessage({ event: "fetch", payload: { args, context: window.name, id: ++opendeck_fetch_count }}, "*");
						return new Promise((resolve, reject) => { opendeck_fetch_promises[opendeck_fetch_count] = { resolve, reject }; });
					};
				</script>
			"#;

fn header(field: &str, value: &str) -> Header {
	Header {
		field: field.parse().unwrap(),
		value: value.parse().unwrap(),
	}
}

fn bind_listener() -> Server {
	let listener = std::net::TcpListener::bind((super::LOOPBACK_HOST, *super::PORT_BASE + 2)).unwrap();

	#[cfg(windows)]
	{
		use std::os::windows::io::AsRawSocket;
		use windows_sys::Win32::Foundation::{HANDLE_FLAG_INHERIT, SetHandleInformation};

		unsafe { SetHandleInformation(listener.as_raw_socket() as _, HANDLE_FLAG_INHERIT, 0) };
	}

	Server::from_listener(listener, None).unwrap()
}

fn respond_daisyui(request: Request) {
	let mut response = Response::from_string(PROPERTY_INSPECTOR_DAISYUI);
	response.add_header(header("Content-Type", "text/css; charset=utf-8"));
	response.add_header(header("Cache-Control", "public, max-age=31536000, immutable"));
	response.add_header(header("Access-Control-Allow-Origin", "*"));
	let _ = request.respond(response);
}

fn respond_not_found(request: Request) {
	let _ = request.respond(Response::empty(404).with_header(header("Access-Control-Allow-Origin", "*")));
}

fn respond_html(request: Request, content: String) {
	let mut response = Response::from_string(content);
	response.add_header(header("Access-Control-Allow-Origin", "*"));
	response.add_header(header("Content-Type", "text/html"));
	let _ = request.respond(response);
}

/// Serves a property inspector page with the theme and the OpenDeck bridge script injected.
///
/// The Svelte frontend cannot call the connectElgatoStreamDeckSocket function on property inspector frames
/// because they are served from a different origin (this plugin asset webserver).
/// Instead, we have to inject a script onto all property inspector frames that receives a message
/// from the Svelte frontend over window.postMessage.
///
/// Additionally, Tauri cannot support window.open as seperate Tauri windows have seperate JavaScript contexts.
/// However, plugin property inspectors expect access to this function.
/// Instead, we have to inject a replacement window.open implementation that creates an IFrame element
/// and requests the Svelte frontend to maximise the property inspector.
async fn respond_property_inspector(request: Request, path: &str) {
	if !matches!(tokio::fs::try_exists(path).await, Ok(true)) {
		return respond_not_found(request);
	}

	let mut content = tokio::fs::read_to_string(path).await.unwrap_or_default();
	inject_property_inspector_theme(&mut content);
	content += PROPERTY_INSPECTOR_SCRIPT;
	respond_html(request, content);
}

async fn respond_property_inspector_child(request: Request, path: &str) {
	if !matches!(tokio::fs::try_exists(path).await, Ok(true)) {
		return respond_not_found(request);
	}

	let mut content = tokio::fs::read_to_string(path).await.unwrap_or_default();
	inject_property_inspector_theme(&mut content);
	content = format!("<script>window.opener ??= window.parent;</script>{content}");
	respond_html(request, content);
}

async fn respond_static_file(request: Request, url: String) {
	if !matches!(tokio::fs::try_exists(&url).await, Ok(true)) {
		return respond_not_found(request);
	}

	let mime_type = mime(&match Path::new(&url).extension() {
		Some(extension) => extension.to_string_lossy().into_owned(),
		None => "html".to_owned(),
	});
	let content_type = header("Content-Type", &mime_type);
	let access_control_allow_origin = header("Access-Control-Allow-Origin", "*");

	if mime_type.starts_with("text/") || mime_type == "image/svg+xml" {
		let mut response = Response::from_string(tokio::fs::read_to_string(url).await.unwrap_or_default());
		response.add_header(access_control_allow_origin);
		response.add_header(content_type);
		let _ = request.respond(response);
		return;
	}

	let Ok(file) = tokio::fs::File::open(url).await else {
		return;
	};
	let mut response = Response::from_file(file.into_std().await);
	response.add_header(access_control_allow_origin);
	response.add_header(content_type);
	let _ = request.respond(response);
}

async fn handle_request(request: Request, prefix: &Path) {
	let mut url = urlencoding::decode(request.url()).unwrap().into_owned();
	if let Some((path, _)) = url.split_once('?') {
		url = path.to_owned();
	}
	if url == PROPERTY_INSPECTOR_DAISYUI_PATH {
		return respond_daisyui(request);
	}
	#[cfg(target_os = "windows")]
	let url = url[1..].replace('/', "\\");

	// Ensure the requested path is within the config directory to prevent unrestricted access to the filesystem.
	let developer = match crate::store::Store::new("settings", prefix, crate::store::Settings::default()) {
		Ok(store) => store.value.developer,
		Err(_) => false,
	};
	if !developer && !Path::new(&url).starts_with(prefix) {
		let _ = request.respond(Response::empty(403));
		return;
	}

	if let Some(path) = url.strip_suffix("|opendeck_property_inspector") {
		respond_property_inspector(request, path).await;
	} else if let Some(path) = url.strip_suffix("|opendeck_property_inspector_child") {
		respond_property_inspector_child(request, path).await;
	} else {
		respond_static_file(request, url).await;
	}
}

/// Start a simple webserver to serve files of plugins that run in a browser environment.
pub async fn init_webserver(prefix: PathBuf) {
	let server = bind_listener();

	for request in server.incoming_requests() {
		handle_request(request, &prefix).await;
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn injects_daisyui_before_the_compatibility_theme() {
		let mut html = "<html><head><title>Inspector</title></head><body></body></html>".to_owned();
		inject_property_inspector_theme(&mut html);

		let daisyui = html.find("opendeck-property-inspector-daisyui").unwrap();
		let compatibility_theme = html.find("opendeck-property-inspector-theme").unwrap();
		assert!(daisyui < compatibility_theme);
		assert!(html.contains(PROPERTY_INSPECTOR_DAISYUI_PATH));
		assert!(html.contains(env!("OPENDECK_DAISYUI_HASH")));
	}

	#[test]
	fn embedded_bundle_contains_core_daisyui_components_and_themes() {
		assert!(PROPERTY_INSPECTOR_DAISYUI.contains("daisyUI"));
		assert!(PROPERTY_INSPECTOR_DAISYUI.contains(".btn"));
		assert!(PROPERTY_INSPECTOR_DAISYUI.contains(".input"));
		assert!(PROPERTY_INSPECTOR_DAISYUI.contains("[data-theme=dark]"));
	}
}
