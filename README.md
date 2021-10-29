# httpstat

![Screenshot](./screenshot.png)

Rust clone of https://github.com/reorx/httpstat.

## Installation

```
$ cargo install --git https://github.com/johnnynotsolucky/rust-httpstat
```

## Usage
```
$ httpstat https://example.com/
$ httpstat --help

USAGE:
    httpstat [FLAGS] [OPTIONS] <url>

FLAGS:
    -h, --help         Prints help information
    -k, --insecure     Allow insecure server connections when using SSL
    -L, --location     Follow redirects
    -o, --save-body    Save response body to a temporary file
    -V, --version      Prints version information
    -v, --verbose      Verbose output

OPTIONS:
    -s, --max-response-size <bytes>    Maximum response size in bytes
        --cacert <ca file>             CA certificate to verify against
    -E, --cert <cert file>             Client certificate file
    -X, --request-method <command>     Specify request method to use [default: GET]
    -d, --data <data>                  HTTP POST data
    -H, --header <headers>...          Pass custom header(s) to server
        --key <key file>               Private key file
        --connect-timeout <millis>     Maximum time allowed for connection

ARGS:
    <url>    URL to work with
```

## Contributing

TODO
