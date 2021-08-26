use eyre::Result;
use nanoid::nanoid;
use std::env;
use std::fs::File;
use std::io::Write;
use std::rc::Rc;
use std::str;
use std::time::Duration;

type ColorFormatter = Box<dyn Fn(String) -> String>;

struct Colors {
    green: ColorFormatter,
    yellow: ColorFormatter,
    cyan: ColorFormatter,
    gray: ColorFormatter,
}

impl Colors {
    fn make_color(c: u8) -> ColorFormatter {
        Box::new(move |s| format!("\x1b[{}m{}\x1b[0m", c, s))
    }

    fn new() -> Self {
        Colors {
            green: Self::make_color(32),
            yellow: Self::make_color(33),
            cyan: Self::make_color(36),
            gray: Self::make_color(38),
        }
    }
}

type TimeFormatter = Box<dyn Fn(Duration) -> String>;

fn make_a_formatter(c: Rc<Colors>) -> TimeFormatter {
    Box::new(move |x| (c.cyan)(format!("{:^7}", format!("{:.0}ms", x.as_millis()))))
}

fn make_b_formatter(c: Rc<Colors>) -> TimeFormatter {
    Box::new(move |x| (c.cyan)(format!("{:<7}", format!("{:.0}ms", x.as_millis()))))
}

fn main() -> Result<()> {
    let url = "https://httpbin.org/get";

    let colors = Rc::new(Colors::new());

    let mut handle = curl::easy::Easy::new();

    handle.url(url)?;
    handle.show_header(true)?;
    handle.verbose(false)?;

    let mut body = Vec::new();
    let mut headers = Vec::new();
    {
        let mut transfer = handle.transfer();
        transfer.write_function(|data| {
            body.extend_from_slice(data);
            Ok(data.len())
        })?;

        transfer.header_function(|header| {
            headers.push(str::from_utf8(header).unwrap().to_string());
            true
        })?;

        transfer.perform()?;
    }

    for (idx, header) in headers.iter().enumerate() {
        if idx == 0 {
            let header_tuple = header.split_once('/').unwrap();
            let header_name: String = header_tuple.0.into();
            let header_value: String = header_tuple.1.into();

            println!(
                "{}{}{}",
                (colors.green)(header_name),
                (colors.gray)("/".into()),
                (colors.cyan)(header_value.trim().replace("\r", "").replace("\n", "")),
            );
        } else if !header.trim().is_empty() {
            let header_tuple: (&str, &str) = header.split_once(':').unwrap();
            let header_name: String = header_tuple.0.into();
            let header_value: String = header_tuple.1.into();
            println!(
                "{}{}",
                (colors.gray)(format!("{}: ", header_name)),
                (colors.cyan)(header_value.trim().replace("\r", "").replace("\n", "")),
            );
        }
    }

    let tmpfile_name = nanoid!(6, &nanoid::alphabet::SAFE); //=> "93ce_Ltuub"
    let tmpfile_path = format!("{}/tmp{}", env::temp_dir().to_str().unwrap(), tmpfile_name);
    let mut tmpfile = File::create(tmpfile_path.clone())?;
    tmpfile.write_all(&body[..])?;
    println!(
        "\n{} stored in {}",
        (colors.green)("Body".to_string()),
        tmpfile_path
    );

    let namelookup_time = handle.namelookup_time()?;
    let connect_time = handle.connect_time()?;
    let pretransfer_time = handle.pretransfer_time()?;
    let starttransfer_time = handle.starttransfer_time()?;
    let total_time = handle.total_time()?;

    let format_a = make_a_formatter(colors.clone());
    let format_b = make_b_formatter(colors);

    let output = if url.starts_with("https") {
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
            a0000 = format_a(namelookup_time),
            a0001 = format_a(connect_time - namelookup_time),
            a0002 = format_a(pretransfer_time - connect_time),
            a0003 = format_a(starttransfer_time - pretransfer_time),
            a0004 = format_a(total_time - starttransfer_time),
            b0000 = format_b(namelookup_time),
            b0001 = format_b(connect_time),
            b0002 = format_b(pretransfer_time),
            b0003 = format_b(starttransfer_time),
            b0004 = format_b(total_time)
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
            a0000 = format_a(namelookup_time),
            a0001 = format_a(connect_time - namelookup_time),
            a0003 = format_a(starttransfer_time - pretransfer_time),
            a0004 = format_a(total_time - starttransfer_time),
            b0000 = format_b(namelookup_time),
            b0001 = format_b(connect_time),
            b0003 = format_b(starttransfer_time),
            b0004 = format_b(total_time)
        )
    };

    println!("{}", output);
    Ok(())
}
