use curl::easy::{Easy, List};
use eyre::Result;
use std::fs;
use std::io::Read;
use std::str;
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
	#[error("{0}")]
	FromStrError(String),
}

pub struct Config {
	pub location: bool,
	pub connect_timeout: Option<Duration>,
	pub request: String,
	pub data: Option<String>,
	pub headers: Option<Vec<String>>,
	pub insecure: bool,
	pub url: String,
	pub verbose: bool,
}

pub struct Header {
	pub name: String,
	pub value: String,
}

impl From<String> for Header {
	fn from(line: String) -> Self {
		let header_tuple: (&str, &str) = line.split_once(':').unwrap();
		Self {
			name: header_tuple.0.into(),
			value: header_tuple.1.trim().into(),
		}
	}
}

pub struct HttpResponseHeader {
	pub http_version: String,
	pub response_code: i32,
	pub response_message: Option<String>,
}

impl From<String> for HttpResponseHeader {
	fn from(line: String) -> Self {
		let cleaned = line.trim().replace("\r", "").replace("\n", "");
		let header_tuple: (&str, &str) = cleaned.split_once('/').unwrap();
		let response_arr: Vec<&str> = header_tuple.1.split(' ').collect();

		let http_version: String = response_arr.get(0).unwrap().to_string();
		let response_code: i32 = response_arr.get(1).unwrap().parse().unwrap();
		let response_message: Option<String> = response_arr.get(2).map(|msg| msg.to_string());

		Self {
			http_version,
			response_code,
			response_message,
		}
	}
}

pub struct Timing {
	pub namelookup_time: Duration,
	pub connect_time: Duration,
	pub pretransfer_time: Duration,
	pub starttransfer_time: Duration,
	pub total_time: Duration,
}

pub struct StatResult {
	pub http_response_header: Option<HttpResponseHeader>,
	pub headers: Vec<Header>,
	pub timing: Timing,
	pub body: Vec<u8>,
}

pub fn httpstat(config: Config) -> Result<StatResult> {
	let mut handle = Easy::new();

	handle.url(&config.url)?;
	handle.show_header(true)?;
	handle.verbose(config.verbose)?;

	if config.insecure {
		handle.ssl_verify_host(false)?;
		handle.ssl_verify_peer(false)?;
	}

	if config.location {
		handle.follow_location(true)?;
	}

	if let Some(connect_timeout) = config.connect_timeout {
		handle.connect_timeout(connect_timeout)?;
	}

	handle.custom_request(&config.request.to_uppercase())?;

	let post_data = if let Some(ref data) = config.data {
		if let Some(data) = data.strip_prefix('@') {
			fs::read_to_string(data)?
		} else {
			data.clone()
		}
	} else {
		"".into()
	};

	if config.data.is_some() {
		handle.post(true)?;
		handle.post_field_size(post_data.len() as u64)?;
	}

	if let Some(config_headers) = config.headers {
		let mut headers = List::new();
		for header in config_headers {
			headers.append(&header)?;
		}
		handle.http_headers(headers)?;
	}

	let mut body = Vec::new();
	let mut header_lines = Vec::new();
	{
		let mut transfer = handle.transfer();

		transfer.read_function(move |into| Ok(post_data.as_bytes().read(into).unwrap()))?;

		transfer.write_function(|data| {
			body.extend_from_slice(data);
			Ok(data.len())
		})?;

		transfer.header_function(|header| {
			let header_str = str::from_utf8(header).unwrap().to_string();
			header_lines.push(header_str);
			true
		})?;

		transfer.perform()?;
	}

	let mut http_response_header: Option<HttpResponseHeader> = None;
	let mut headers: Vec<Header> = Vec::new();

	let header_iter = header_lines
		.iter()
		.map(|line| line.replace("\r", "").replace("\n", ""))
		.filter(|line| !line.is_empty());

	for line in header_iter {
		if line.to_uppercase().starts_with("HTTP/") {
			http_response_header = Some(HttpResponseHeader::from(line.to_string()));
		} else {
			headers.push(Header::from(line.to_string()));
		}
	}

	Ok(StatResult {
		http_response_header,
		headers,
		body,
		timing: Timing {
			namelookup_time: handle.namelookup_time()?,
			connect_time: handle.connect_time()?,
			pretransfer_time: handle.pretransfer_time()?,
			starttransfer_time: handle.starttransfer_time()?,
			total_time: handle.total_time()?,
		},
	})
}
