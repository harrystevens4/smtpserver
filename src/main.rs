mod smtp;
mod email;

use crate::smtp::recieve_emails;

use std::net::{TcpListener};
use std::process::ExitCode;

fn main() -> ExitCode {
	println!("awaiting connections");
	//====== setup listener ======
	let listener = match TcpListener::bind("0.0.0.0:25"){
		Ok(l) => l, Err(e) => {
			eprintln!("Could not start listener on port 25: {e}");
			return ExitCode::FAILURE;
		}
	};
	//====== accept incomming connections ======
	loop {
		//accept
		let (socket,addr) = match listener.accept() {
			Ok(s) => s,
			Err(e) => {
				eprintln!("Error while connecting to client: {e}");
				continue;
			}
		};
		println!("==> new connection [{addr}]");
		//pass connection to receive function
		let emails = match recieve_emails(socket){
			Ok(emails) => emails,
			Err(e) => {
				eprintln!("receive_email: {}",e);
				continue;
			},
		};
	}
}
