mod smtp;

use crate::smtp::recieve_emails;
use maildb::MailDB;
use args::Args;
use std::env;

use std::net::{TcpListener};
use std::process::ExitCode;

fn main() -> ExitCode {
	//====== process arguments ======
	let cmd_args = Args::gather(&[
		('h', Some("help"),    false),
		('f', Some("db-path"), true ),
	]);
	if cmd_args.has('h'){
		print_help();
		return ExitCode::SUCCESS;
	}
	let db_path = cmd_args.get_value('f').unwrap_or(String::from("/var/mail/mail.db"));
	//====== database ======
	println!("Connecting to mail database...");
	let mail_db = match MailDB::open(&db_path){
		Ok(db) => db,
		Err(err) => {
			eprintln!("Could not open mail databse: {err}");
			return ExitCode::FAILURE;
		}
	};
	println!("Awaiting connections");
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
		println!("========> new connection [{addr}] <========");
		//pass connection to receive function
		let emails = match recieve_emails(socket){
			Ok(emails) => emails,
			Err(e) => {
				eprintln!("receive_email: {}",e);
				continue;
			},
		};
		for email in emails {
			println!("====== new email ======");
			println!("===> Senders: {:?}",email.senders);
			println!("===> Recipients: {:?}",email.recipients);
			println!("{}",email.data);
			//store the email in the databse
			if let Err(e) = mail_db.store_email(email){
				eprintln!("Error storing mail: {e}");
			};
		}
	}
}

fn print_help(){
	let name = env::args().next().unwrap_or("smtpserver".to_string());
	println!("Usage: {name} [options]");
	println!("Options:");
	println!("	-h, --help    : Show this help message");
	println!("	-f, --db-path : Path of the mail database to use");
}
