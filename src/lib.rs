use anyhow::{anyhow, Result};
use curl::easy::{Easy2, Handler, List, ReadError, WriteError};
use curl::multi::{Easy2Handle, Multi};
use serde::{Deserialize, Serialize};
use std::fs;
use std::future::Future;
use std::io::Read;
use std::pin::Pin;
use std::str;
use std::task::{Context, Poll};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct Config {
	pub location: bool,
	pub connect_timeout: Option<Duration>,
	pub request: String,
	pub data: Option<String>,
	pub headers: Option<Vec<String>>,
	pub insecure: bool,
	pub url: String,
	pub verbose: bool,
	pub max_response_size: Option<usize>,
}

impl Default for Config {
	fn default() -> Self {
		Self {
			location: false,
			connect_timeout: None,
			request: "GET".into(),
			data: None,
			headers: None,
			insecure: false,
			url: "".into(),
			verbose: false,
			max_response_size: None,
		}
	}
}

#[derive(Deserialize, Serialize, Debug, Clone)]
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

#[derive(Deserialize, Serialize, Debug, Clone)]
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

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Timing {
	pub namelookup_time: Duration,
	pub connect_time: Duration,
	pub pretransfer_time: Duration,
	pub starttransfer_time: Duration,
	pub total_time: Duration,
	pub dns_resolution_time: Duration,
	pub tcp_connection_time: Duration,
	pub tls_connection_time: Duration,
	pub server_processing_time: Duration,
	pub content_transfer_time: Duration,
}

impl Timing {
	pub fn new(handle: &mut Easy2Handle<Collector>) -> Self {
		let namelookup_time = handle.namelookup_time().unwrap();
		let connect_time = handle.connect_time().unwrap();
		let pretransfer_time = handle.pretransfer_time().unwrap();
		let starttransfer_time = handle.starttransfer_time().unwrap();
		let total_time = handle.total_time().unwrap();
		let dns_resolution_time = namelookup_time;
		let tcp_connection_time = connect_time - namelookup_time;
		let tls_connection_time = pretransfer_time - connect_time;
		let server_processing_time = starttransfer_time - pretransfer_time;
		let content_transfer_time = total_time - starttransfer_time;

		Self {
			namelookup_time,
			connect_time,
			pretransfer_time,
			starttransfer_time,
			total_time,
			dns_resolution_time,
			tcp_connection_time,
			tls_connection_time,
			server_processing_time,
			content_transfer_time,
		}
	}
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct StatResult {
	pub http_version: String,
	pub response_code: i32,
	pub response_message: Option<String>,
	pub headers: Vec<Header>,
	pub timing: Timing,
	pub body: Vec<u8>,
}

pub struct Collector<'a> {
	config: &'a Config,
	headers: &'a mut Vec<u8>,
	data: &'a mut Vec<u8>,
}

impl<'a> Collector<'a> {
	pub fn new(config: &'a Config, data: &'a mut Vec<u8>, headers: &'a mut Vec<u8>) -> Self {
		Self {
			config,
			data,
			headers,
		}
	}
}

// TODO shouldn't need to call this twice
fn get_upload_data(data: &Option<String>) -> Result<Option<String>> {
	match data {
		Some(ref data) => match data.strip_prefix('@') {
			Some(data) => match fs::read_to_string(data) {
				Ok(data) => Ok(Some(data)),
				Err(error) => Err(error.into()),
			},
			None => Ok(Some(data.clone())),
		},
		None => Ok(None),
	}
}

impl<'a> Handler for Collector<'a> {
	fn write(&mut self, data: &[u8]) -> Result<usize, WriteError> {
		self.data.extend_from_slice(data);
		if let Some(ref max_response_size) = self.config.max_response_size {
			if self.data.len() > *max_response_size {
				return Ok(0);
			}
		}
		Ok(data.len())
	}

	fn read(&mut self, into: &mut [u8]) -> Result<usize, ReadError> {
		match get_upload_data(&self.config.data) {
			Ok(data) => match data {
				Some(data) => Ok(data.as_bytes().read(into).unwrap()),
				None => Ok(0),
			},
			Err(_error) => Err(ReadError::Abort),
		}
	}

	fn header(&mut self, data: &[u8]) -> bool {
		self.headers.extend_from_slice(data);
		true
	}
}

pub struct HttpstatFuture<'a>(&'a Multi);

impl<'a> Future for HttpstatFuture<'a> {
	type Output = Result<()>;

	fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
		match self.0.perform() {
			Ok(running) => {
				if running > 0 {
					context.waker().wake_by_ref();
					Poll::Pending
				} else {
					Poll::Ready(Ok(()))
				}
			}
			Err(error) => Poll::Ready(Err(error.into())),
		}
	}
}

// TODO now make a sync version
pub async fn httpstat(config: &Config) -> Result<StatResult> {
	let mut body = Vec::new();
	let mut headers = Vec::new();
	let mut handle = Easy2::new(Collector::new(config, &mut body, &mut headers));

	handle.url(&config.url)?;
	handle.show_header(false)?;
	handle.progress(true)?;
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

	if config.data.is_some() {
		handle.post(true)?;
		handle.post_field_size(
			get_upload_data(&config.data)?
				.unwrap_or_else(|| "".into())
				.len() as u64,
		)?;
	}

	if let Some(config_headers) = &config.headers {
		let mut headers = List::new();
		for header in config_headers {
			headers.append(header)?;
		}
		handle.http_headers(headers)?;
	}

	let multi = Multi::new();
	let mut handle = multi.add2(handle)?;
	HttpstatFuture(&multi).await?;

	// hmmm
	let mut transfer_result: Result<()> = Ok(());
	multi.messages(|m| {
		if let Ok(()) = transfer_result {
			if let Some(Err(error)) = m.result_for2(&handle) {
				if error.is_write_error() {
					transfer_result = Err(anyhow!("Maximum response size reached"));
				} else {
					transfer_result = Err(error.into());
				}
			}
		}
	});
	transfer_result?;

	let timing = Timing::new(&mut handle);
	// Force handler to drop so we can access the body references held by the collector
	drop(handle);

	let header_lines = str::from_utf8(&headers[..])?.lines();

	let mut http_response_header: Option<HttpResponseHeader> = None;
	let mut headers: Vec<Header> = Vec::new();

	let header_iter = header_lines
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
		http_version: http_response_header
			.as_ref()
			.map_or_else(|| "Unknown".into(), |h| h.http_version.clone()),
		response_code: http_response_header
			.as_ref()
			.map_or(-1, |h| h.response_code),
		response_message: http_response_header
			.as_ref()
			.and_then(|h| h.response_message.clone()),
		headers,
		body,
		timing,
	})
}
