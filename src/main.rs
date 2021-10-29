use anyhow::Result;
use futures::executor::block_on;
use nanoid::nanoid;
use std::env;
use std::fs::{self, File};
use std::io::Write;
use std::time::Duration;
use structopt::StructOpt;

use httpstat::{httpstat, Config, Header};

#[derive(Debug, Clone, StructOpt)]
#[structopt()]
struct Opt {
	/// Follow redirects
	#[structopt(short = "L", long = "location")]
	location: bool,

	/// Maximum time allowed for connection
	#[structopt(name = "millis", long = "connect-timeout")]
	connect_timeout: Option<u64>,

	/// Specify request method to use
	#[structopt(
		name = "command",
		short = "X",
		long = "request-method",
		default_value = "GET"
	)]
	request_method: String,

	/// HTTP POST data
	#[structopt(short = "d", long = "data")]
	data: Option<String>,

	/// Pass custom header(s) to server
	#[structopt(short = "H", long = "header")]
	headers: Vec<Header>,

	/// Allow insecure server connections when using SSL
	#[structopt(short = "k", long = "insecure")]
	insecure: bool,

	/// Client certificate file
	#[structopt(name = "cert file", short = "E", long = "cert")]
	client_cert: Option<String>,

	/// Private key file
	#[structopt(name = "key file", long = "key")]
	client_key: Option<String>,

	/// CA certificate to verify against
	#[structopt(name = "ca file", long = "cacert")]
	ca_cert: Option<String>,

	/// Save response body to a temporary file
	#[structopt(short = "o", long = "save-body")]
	save_body: bool,

	/// Verbose output
	#[structopt(short = "v", long = "verbose")]
	verbose: bool,

	/// Maximum response size in bytes
	#[structopt(name = "bytes", short = "s", long = "max-response-size")]
	max_response_size: Option<usize>,

	/// URL to work with
	url: String,
}

fn get_upload_data(data: Option<String>) -> Result<Option<String>> {
	match data {
		Some(data) => match data.strip_prefix('@') {
			Some(data) => match fs::read_to_string(data) {
				Ok(data) => Ok(Some(data)),
				Err(error) => Err(error.into()),
			},
			None => Ok(Some(data)),
		},
		None => Ok(None),
	}
}

impl From<Opt> for Config {
	fn from(opt: Opt) -> Self {
		Self {
			location: opt.location,
			connect_timeout: opt.connect_timeout.map(Duration::from_millis),
			request_method: opt.request_method.into(),
			data: opt.data,
			headers: opt.headers,
			insecure: opt.insecure,
			client_cert: opt.client_cert,
			client_key: opt.client_key,
			ca_cert: opt.ca_cert,
			url: opt.url,
			verbose: opt.verbose,
			max_response_size: opt.max_response_size,
		}
	}
}

type ColorFormatter = fn(String) -> String;

macro_rules! make_color {
	($e:expr) => {
		|s: String| format!("\x1b[{}m{}\x1b[0m", $e, s)
	};
}

macro_rules! make_color_formatter {
	($color:expr, $format: expr) => {
		|duration: Duration| $color(format!($format, format!("{:.0}ms", duration.as_millis())))
	};
}

const GREEN: ColorFormatter = make_color!(32);
const YELLOW: ColorFormatter = make_color!(33);
const CYAN: ColorFormatter = make_color!(36);
const GRAY: ColorFormatter = make_color!(38);

fn execute() -> Result<()> {
	let mut opt = Opt::from_args();
	opt.data = get_upload_data(opt.data)?;

	let result = block_on(httpstat(&Config::from(opt.clone())))?;

	println!(
		"{}{}{}",
		GREEN("HTTP".into()),
		GRAY("/".into()),
		CYAN(format!(
			"{} {} {}",
			result.http_version,
			result.response_code,
			if let Some(msg) = result.response_message {
				msg
			} else {
				"".into()
			},
		)),
	);

	for header in result.headers.iter() {
		println!(
			"{}{}",
			GRAY(format!("{}: ", header.name)),
			CYAN(header.value.to_owned()),
		);
	}

	if opt.save_body {
		let tmpfile_name = nanoid!(6, &nanoid::alphabet::SAFE); //=> "93ce_Ltuub"
		let tmpfile_path = format!("{}/tmp{}", env::temp_dir().to_str().unwrap(), tmpfile_name);
		let mut tmpfile = File::create(&tmpfile_path)?;
		tmpfile.write_all(&result.body[..])?;
		println!(
			"\n{} stored in {}",
			GREEN("Body".to_string()),
			&tmpfile_path
		);
	}

	let format_a = make_color_formatter!(CYAN, "{:^7}"); //make_a_formatter();
	let format_b = make_color_formatter!(CYAN, "{:<7}"); //make_b_formatter();

	let output = if opt.url.starts_with("https") {
		format!(
			r#"
  DNS Lookup   TCP Connection   TLS Handshake   Server Processing   Content Transfer
[   {a0000}  |     {a0001}    |    {a0002}    |      {a0003}      |      {a0004}     ]
             |                |               |                   |                  |
    namelookup:{b0000}        |               |                   |                  |
                        connect:{b0001}       |                   |                  |
                                    pretransfer:{b0002}           |                  |
                                                      starttransfer:{b0003}          |
                                                                                 total:{b0004}
"#,
			a0000 = format_a(result.timing.dns_resolution),
			a0001 = format_a(result.timing.tcp_connection),
			a0002 = format_a(result.timing.tls_connection),
			a0003 = format_a(result.timing.server_processing),
			a0004 = format_a(result.timing.content_transfer),
			b0000 = format_b(result.timing.namelookup),
			b0001 = format_b(result.timing.connect),
			b0002 = format_b(result.timing.pretransfer),
			b0003 = format_b(result.timing.starttransfer),
			b0004 = format_b(result.timing.total)
		)
	} else {
		format!(
			r#"
  DNS Lookup   TCP Connection   Server Processing   Content Transfer
[   {a0000}  |     {a0001}    |      {a0003}      |      {a0004}     ]
             |                |                   |                  |
    namelookup:{b0000}        |                   |                  |
                        connect:{b0001}           |                  |
                                      starttransfer:{b0003}          |
                                                                 total:{b0004}
"#,
			a0000 = format_a(result.timing.dns_resolution),
			a0001 = format_a(result.timing.tcp_connection),
			a0003 = format_a(result.timing.server_processing),
			a0004 = format_a(result.timing.content_transfer),
			b0000 = format_b(result.timing.namelookup),
			b0001 = format_b(result.timing.connect),
			b0003 = format_b(result.timing.starttransfer),
			b0004 = format_b(result.timing.total)
		)
	};

	println!("{}", output);
	Ok(())
}

fn main() {
	if let Err(err) = execute() {
		println!("{}", YELLOW(err.to_string()));
	}
}
