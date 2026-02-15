mod pop3;

use pop3::*;

use std::net::{TcpListener,TcpStream};
use std::process::ExitCode;
use args::Args;
use std::{env,error};
use maildb::MailDB;

fn main() -> ExitCode {
	//====== process cmdline arguments ======
	let arguments = Args::gather(&[
		('h', Some("help"),    false),
		('f', Some("db-path"), true ),
	]);
	if arguments.has('h'){
		print_help();
		return ExitCode::SUCCESS;
	}
	let db_path = arguments.get_value('f').unwrap_or(String::from("/var/mail/mail.db"));
	//====== database ======
	let mail_db = match MailDB::open(&db_path){
		Ok(db) => db,
		Err(err) => {
			eprintln!("Could not open mail databse: {err}");
			return ExitCode::FAILURE;
		}
	};
	//====== listen for tcp connections ======
	let listener = match TcpListener::bind("0.0.0.0:110"){
		Ok(l) => l, Err(e) => {
			eprintln!("Couldn't bind to port 110: {e}");
			return ExitCode::FAILURE;
		}
	};
	//====== accept connections ======
	loop {
		let (connection,address) = match listener.accept(){
			Ok(c) => c, Err(e) => {
				eprintln!("Could not accept connection: {e}");
				return ExitCode::FAILURE;
			},
		};
		println!("===> new connection: [{address}] <===");
		match handle_connection(connection,&mail_db){
			Ok(_) => (),
			Err(e) => {
				eprintln!("handle_connection: {e}");
				continue;
			}
		}
	}
}

fn handle_connection(mut connection: TcpStream, mail_db: &MailDB) -> Result<(),Box<dyn error::Error>> {
	println!("shaking hands...");
	pop3_handshake(&mut connection)?;
	println!("authenticating...");
	let user = pop3_authenticate(&mut connection,
		|user|{
			Ok(mail_db.check_user_exists(user)?)
		},
		|user,password|{
			Ok(mail_db.verify_user_password(user,password)?)
		}
	)?;
	println!("processing transactions...");
	pop3_process_transactions(&mut connection,&mail_db,&user)?;
	Ok(())
}

fn print_help(){
	let name = env::args().next().unwrap_or("pop3server".to_string());
	println!("Usage: {name} [options]");
	println!("Options:");
	println!("	-h, --help    : Show this help message");
	println!("	-f, --db-path : Path of the mail database to use");
}
