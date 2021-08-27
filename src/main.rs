use eyre::Result;
use nanoid::nanoid;
use std::env;
use std::fs::File;
use std::io::Write;
use std::time::Duration;
use structopt::StructOpt;

use httpstat::{httpstat, Config};

#[derive(Debug, Clone, StructOpt)]
#[structopt()]
struct Opt {
	#[structopt(short = "L", long = "location")]
	/// Follow redirects
	location: bool,

	#[structopt(long = "connect-timeout")]
	/// <seconds> Maximum time allowed for connection
	connect_timeout: Option<i32>,

	// TODO make enum
	#[structopt(short = "X", long = "request", default_value = "GET")]
	/// Specify request command to use
	request: String,

	#[structopt(short = "d", long = "data")]
	/// HTTP POST data
	data: Option<String>,

	#[structopt(short = "H", long = "header")]
	/// Pass custom header(s) to server
	headers: Option<Vec<String>>,

	#[structopt(short = "k", long = "insecure")]
	/// Allow insecure server connections when using SSL
	insecure: bool,

	/// URL to work with
	url: String,

	#[structopt(short = "o", long = "save-body")]
	save_body: bool,

	#[structopt(short = "v", long = "verbose")]
	/// Verbose output
	verbose: bool,
}

impl From<Opt> for Config {
	fn from(opt: Opt) -> Self {
		Self {
			location: opt.location,
			connect_timeout: opt.connect_timeout,
			request: opt.request,
			data: opt.data,
			headers: opt.headers,
			insecure: opt.insecure,
			url: opt.url,
			verbose: opt.verbose,
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
	let opt = Opt::from_args();
	let result = httpstat(Config::from(opt.clone()))?;

	if let Some(http_response_header) = result.http_response_header {
		println!(
			"{}{}{}",
			GREEN("HTTP".into()),
			GRAY("/".into()),
			CYAN(format!(
				"{} {} {}",
				http_response_header.http_version,
				http_response_header.response_code,
				if let Some(msg) = http_response_header.response_message {
					msg
				} else {
					"".into()
				},
			)),
		);
	}

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
		let mut tmpfile = File::create(tmpfile_path.clone())?;
		tmpfile.write_all(&result.body[..])?;
		println!("\n{} stored in {}", GREEN("Body".to_string()), tmpfile_path);
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
			a0000 = format_a(result.timing.namelookup_time),
			a0001 = format_a(result.timing.connect_time - result.timing.namelookup_time),
			a0002 = format_a(result.timing.pretransfer_time - result.timing.connect_time),
			a0003 = format_a(result.timing.starttransfer_time - result.timing.pretransfer_time),
			a0004 = format_a(result.timing.total_time - result.timing.starttransfer_time),
			b0000 = format_b(result.timing.namelookup_time),
			b0001 = format_b(result.timing.connect_time),
			b0002 = format_b(result.timing.pretransfer_time),
			b0003 = format_b(result.timing.starttransfer_time),
			b0004 = format_b(result.timing.total_time)
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
			a0000 = format_a(result.timing.namelookup_time),
			a0001 = format_a(result.timing.connect_time - result.timing.namelookup_time),
			a0003 = format_a(result.timing.starttransfer_time - result.timing.pretransfer_time),
			a0004 = format_a(result.timing.total_time - result.timing.starttransfer_time),
			b0000 = format_b(result.timing.namelookup_time),
			b0001 = format_b(result.timing.connect_time),
			b0003 = format_b(result.timing.starttransfer_time),
			b0004 = format_b(result.timing.total_time)
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
