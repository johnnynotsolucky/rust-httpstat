use anyhow::{anyhow, Result};
use core::fmt;
use curl::easy::{Easy2, Handler, List, ReadError, WriteError};
use curl::multi::{Easy2Handle, Multi};
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::io::Read;
use std::pin::Pin;
use std::str::{self, FromStr};
use std::task::{Context, Poll};
use std::time::Duration;

#[derive(Debug, Clone)]
pub enum RequestMethod {
	// TODO Support CONNECT - https://curl.se/libcurl/c/CURLOPT_HTTPPROXYTUNNEL.html
	// Connect,
	Delete,
	Get,
	Head,
	Options,
	Patch,
	Post,
	Put,
	Trace,
	Custom(String),
}

impl<'a> From<&'a RequestMethod> for &'a str {
	fn from(request_method: &'a RequestMethod) -> &'a str {
		match request_method {
			// RequestMethod::Connect => "CONNECT",
			RequestMethod::Delete => "DELETE",
			RequestMethod::Get => "GET",
			RequestMethod::Head => "HEAD",
			RequestMethod::Options => "OPTIONS",
			RequestMethod::Patch => "PATCH",
			RequestMethod::Post => "POST",
			RequestMethod::Put => "PUT",
			RequestMethod::Trace => "TRACE",
			RequestMethod::Custom(request_method) => request_method,
		}
	}
}

impl From<String> for RequestMethod {
	fn from(request_method: String) -> Self {
		let request_method = request_method.to_uppercase();
		match request_method.as_str() {
			// "CONNECT" => Self::Connect,
			"DELETE" => RequestMethod::Delete,
			"GET" => RequestMethod::Get,
			"HEAD" => RequestMethod::Head,
			"OPTIONS" => RequestMethod::Options,
			"PATCH" => RequestMethod::Patch,
			"POST" => RequestMethod::Post,
			"PUT" => RequestMethod::Put,
			"TRACE" => RequestMethod::Trace,
			_ => Self::Custom(request_method),
		}
	}
}

#[derive(Debug, Clone)]
pub struct Config {
	pub location: bool,
	pub connect_timeout: Option<Duration>,
	pub request_method: RequestMethod,
	pub data: Option<String>,
	pub headers: Option<Vec<Header>>,
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
			request_method: RequestMethod::Get,
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

impl fmt::Display for Header {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{}: {}", self.name, self.value)
	}
}

impl FromStr for Header {
	type Err = anyhow::Error;
	fn from_str(s: &str) -> Result<Self, Self::Err> {
		match s.split_once(':') {
			Some(header_tuple) => Ok(Self {
				name: header_tuple.0.into(),
				value: header_tuple.1.trim().into(),
			}),
			None => Err(anyhow!("Invalid header \"{}\"", s)),
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
	pub namelookup: Duration,
	pub connect: Duration,
	pub pretransfer: Duration,
	pub starttransfer: Duration,
	pub total: Duration,
	pub dns_resolution: Duration,
	pub tcp_connection: Duration,
	pub tls_connection: Duration,
	pub server_processing: Duration,
	pub content_transfer: Duration,
}

impl Timing {
	pub fn new(handle: &mut Easy2Handle<Collector>) -> Self {
		let namelookup = handle.namelookup_time().unwrap();
		let connect = handle.connect_time().unwrap();
		let pretransfer = handle.pretransfer_time().unwrap();
		let starttransfer = handle.starttransfer_time().unwrap();
		let total = handle.total_time().unwrap();
		let dns_resolution = namelookup;
		let tcp_connection = connect - namelookup;
		let tls_connection = pretransfer - connect;
		let server_processing = starttransfer - pretransfer;
		let content_transfer = total - starttransfer;

		Self {
			namelookup,
			connect,
			pretransfer,
			starttransfer,
			total,
			dns_resolution,
			tcp_connection,
			tls_connection,
			server_processing,
			content_transfer,
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
		match &self.config.data {
			Some(data) => Ok(data.as_bytes().read(into).unwrap()),
			None => Ok(0),
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

	let data_len = config.data.as_ref().map(|data| data.len() as u64);

	let request_method = &config.request_method;
	match request_method {
		RequestMethod::Put => {
			handle.upload(true)?;
			if let Some(data_len) = data_len {
				handle.in_filesize(data_len)?;
			}
		}
		RequestMethod::Get => handle.get(true)?,
		RequestMethod::Head => handle.nobody(true)?,
		RequestMethod::Post => handle.post(true)?,
		_ => handle.custom_request(request_method.into())?,
	}

	// Set post_field_size for anything other than a PUT request if the user has passed in data.
	// Note: https://httpwg.org/specs/rfc7231.html#method.definitions
	// > A payload within a {METHOD} request message has no defined semantics; sending a payload
	// > body on a {METHOD} request might cause some existing implementations to reject the
	// > request.
	if data_len.is_some() && !matches!(request_method, RequestMethod::Put) {
		handle.post_field_size(data_len.unwrap())?;
	}

	if let Some(config_headers) = &config.headers {
		let mut headers = List::new();
		for header in config_headers {
			headers.append(&header.to_string())?;
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
		} else if let Ok(header) = Header::from_str(&line) {
			headers.push(header);
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
