use smtp::{recieve_emails,SMTPServerConfig};
use maildb::{MailDB,Email};
use args::Args;
use std::env;

use std::net::{TcpListener};
use std::process::ExitCode;
use std::time::Duration;
use std::io::ErrorKind;
use std::io;
use std::sync::Arc;
use std::thread;

fn main() -> ExitCode {
	let mut config = SMTPServerConfig::default();
	//====== process arguments ======
	let cmd_args = Args::gather(&[
		('h', Some("help"),    false),
		('f', Some("db-path"), true ),
		('T', Some("timeout"), true ),
	]);
	if cmd_args.has('h'){
		print_help();
		return ExitCode::SUCCESS;
	}
	//database path
	let db_path = cmd_args.get_value('f').unwrap_or(String::from("/var/mail/mail.db"));
	//socket timeout
	if let Some(timeout_ms_str) = cmd_args.get_value('T') {
		let Ok(timeout_ms) = timeout_ms_str.parse()
		else {
			eprintln!("Could not parse timeout");
			return ExitCode::FAILURE;
		};
		config.set_timeout(Duration::from_millis(timeout_ms));
	}
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
	let thread_pool_size = 10;
	let mut thread_pool: Vec<thread::JoinHandle<Option<Vec<Email>>>> = vec![];
	//Atomic reference counted thingies
	let config_arc = Arc::new(config);
	loop {
		//====== wait for available slot in thread pool ======
		loop {
			//join any completed threads
			for i in 0..(thread_pool.len()) {
				if thread_pool[i].is_finished() {
					//if the thread panicked panic here
					if let Some(emails) = thread_pool.remove(i).join().unwrap() {
						//threads return emails they received
						for email in emails {
							println!("====== new email ======");
							println!("===> Senders: {:?}",email.senders_vec());
							println!("===> Recipients: {:?}",email.recipients_vec());
							println!("{}",email.data());
							//store the email in the databse
							if let Err(e) = mail_db.store_email(email){
								eprintln!("Error storing mail: {e}");
							};
						}
					}
					break;
				}
			}
			if thread_pool.len() < thread_pool_size {break}
			//wait for threads to free up
			thread::sleep(Duration::from_millis(100));
		}
		//====== wait for a connection ======
		let (socket,addr) = match listener.accept() {
			Ok(s) => s,
			Err(e) => {
				eprintln!("Error while connecting to client: {e}");
				continue;
			}
		};
		println!("========> new connection [{addr}] <========");
		//====== process the connection ======
		//clone any Arcs needed
		let config_arc_clone = config_arc.clone();
		//start the thread
		thread_pool.push(thread::spawn(move ||{
			let config = config_arc_clone;
			//set timeout
			let _ = socket
				.set_read_timeout(Some(config.timeout()))
				.and_then(|_| socket.set_write_timeout(Some(config.timeout())))
				.inspect_err(|e| eprintln!("Error setting socket timeout: {e}"));
			//pass connection to receive function
			let emails = match recieve_emails(socket,&config){
				Ok(emails) => emails,
				//WouldBlock is actualy connection timed out so lets make that clear
				Err(e) if e.is::<io::Error>() => {
					match e.downcast_ref().map(|e: &io::Error| e.kind()) {
						Some(ErrorKind::WouldBlock) => {
							eprintln!("Connection timed out");
						},
						_ => (),
					}
					return None;
				},
				Err(e) => {
					eprintln!("receive_email: {}",e);
					return None;
				},
			};
			Some(emails)
		}));
	}
}

fn print_help(){
	let name = env::args().next().unwrap_or("smtpserver".to_string());
	println!("Usage: {name} [options]");
	println!("Options:");
	println!("	-h, --help    : Show this help message");
	println!("	-f, --db-path : Path of the mail database to use");
	println!("	-T, --timeout : Connection timeout in milliseconds");
}
