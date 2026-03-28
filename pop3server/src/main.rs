mod pop3;

use pop3::*;

use std::net::{TcpListener,TcpStream};
use std::process::ExitCode;
use args::Args;
use std::{env,error};
use maildb::MailDB;
use std::time::Duration;
use std::io;
use std::io::ErrorKind;

fn main() -> ExitCode {
	let mut pop3_config = POP3Config::default();
	//====== process cmdline arguments ======
	let arguments = Args::gather(&[
		('h', Some("help"),    false),
		('f', Some("db-path"), true ),
		('k', Some("private-key"), true ),
		('c', Some("certificate"), true ),
		('t', Some("enable-tls"), false),
		('T', Some("timeout"), true),
	]);
	if arguments.has('h'){
		print_help();
		return ExitCode::SUCCESS;
	}
	let db_path = arguments.get_value('f').unwrap_or(String::from("/var/mail/mail.db"));
	pop3_config.tls_private_key = arguments.get_value('k');
	pop3_config.tls_certs = arguments.get_value('c');
	pop3_config.tls_enabled = arguments.has('t');
	if let Some(timeout_ms_str) = arguments.get_value('T') {
		let Ok(timeout_ms) = timeout_ms_str.parse::<u64>()
		else {
			eprintln!("Could not parse timeout");
			return ExitCode::FAILURE;
		};
		pop3_config.set_timeout(Duration::from_millis(timeout_ms));
	}
	if pop3_config.tls_enabled {
		if pop3_config.tls_private_key.is_none() || pop3_config.tls_certs.is_none() {
			eprintln!("private key and certificate must be provided for tls");
			return ExitCode::FAILURE;
		}
	}
	//====== database ======
	println!("loading database...");
	let mail_db = match MailDB::open(&db_path){
		Ok(db) => db,
		Err(err) => {
			eprintln!("Could not open mail databse: {err}");
			return ExitCode::FAILURE;
		}
	};
	//====== listen for tcp connections ======
	println!("listening on port 110...");
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
		//set timeouts
		let timeout_result = connection.set_read_timeout(Some(pop3_config.timeout()))
			.and_then(|_| connection.set_write_timeout(Some(pop3_config.timeout())));
		if let Err(e) = timeout_result {
			eprintln!("Could not set socket timeout: {e}");
			return ExitCode::FAILURE;
		}
		println!("===> new connection: [{address}] <===");
		match handle_connection(connection,&mail_db,&pop3_config){
			Ok(_) => (),
			Err(e) => {
				if let Some(io_error) = e.downcast_ref::<io::Error>(){
					//client timed out (more understandable than Resource temprarily unavailable)
					if io_error.kind() == ErrorKind::WouldBlock {
						eprintln!("handle_connection: Connection timed out");
						continue;
					}
				}
				eprintln!("handle_connection: {e}");
				continue;
			}
		}
	}
}

fn handle_connection(mut connection: TcpStream, mail_db: &MailDB, pop3_config: &POP3Config) -> Result<(),Box<dyn error::Error>> {
	println!("shaking hands...");
	pop3_handshake(&mut connection)?;
	println!("authenticating...");
	let (user,mut connection) = pop3_authenticate(connection,pop3_config,
		|user|{
			Ok(mail_db.check_user_exists(user)?)
		},
		|user,password|{
			Ok(mail_db.verify_user_password(user,password)?)
		}
	)?;
	println!("processing transactions...");
	pop3_process_transactions(&mut *connection,&mail_db,&user)?;
	Ok(())
}

fn print_help(){
	let name = env::args().next().unwrap_or("pop3server".to_string());
	println!("Usage: {name} [options]");
	println!("Options:");
	println!("	-h, --help               : Show this help message");
	println!("	-f, --db-path            : Path of the mail database to use");
	println!("	-t, --enable-tls         : Enables STARTTLS support");
	println!("	-k, --private-key <path> : Specifies the private key pemfile to use for tls");
	println!("	-c, --certificate <path> : Path of tls certificate");
	println!("	-T, --timeout            : Socket timeout in milliseconds");
}
