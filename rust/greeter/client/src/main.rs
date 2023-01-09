extern crate grpc;
extern crate helloworld_rust_grpc;

use grpc::ClientStubExt;
use std::env;
use std::str::FromStr;

use helloworld_rust_grpc::*;

fn parse_args() -> (String, u16) {
    let mut name = "world".to_owned();
    let mut port = 50051;
    for arg in env::args().skip(1) {
        if let Some(argp) = arg.strip_prefix("-p=") {
            port = u16::from_str(argp).unwrap()
        } else {
            name = arg.to_owned();
        }
    }
    (name, port)
}

fn main() {
    let (name, port) = parse_args();
    let client = GreeterClient::new_plain("::1", port, Default::default()).unwrap();
    let mut req = HelloRequest::new();
    req.set_name(name);
    let resp = client.say_hello(grpc::RequestOptions::new(), req);
    println!("{:?}", resp.wait());
}
