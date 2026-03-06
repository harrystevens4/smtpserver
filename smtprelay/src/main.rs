use maildb::Email;
mod emailqueue;

use crate::emailqueue::EmailQueue;
use smtp::{send_emails,recieve_emails};
use args::Args;

use std::str::FromStr;
use std::io;
use std::net::{TcpStream,TcpListener,ToSocketAddrs};
use std::process::ExitCode;
use std::thread;
use std::time::{Duration,SystemTime};
use std::error::Error;
use domain::resolv::stub::StubResolver;
use domain::base::iana::{Rtype};
use domain::base::name::Name;
use domain::rdata::rfc1035::Mx;
use std::io::Error as IoError;

use rustls::{ClientConfig,StreamOwned,RootCertStore,ClientConnection};
use rustls_pki_types::{ServerName};

enum FailureType<E> {
	Temporary(E),
	Permanent(E),
}

fn main() -> ExitCode {
	//====== process command line arguments ======
	let args = Args::gather(&[
		('h', Some("help"),         false ),
		('p', Some("port"),         true  ),
		('f', Some("db-path"),      true  ),
		('r', Some("retry-window"), true  ),
	]);
	if args.has('h') {
		print_help();
		return ExitCode::SUCCESS
	}
	let port = args.get_value('p').and_then(|p| p.parse().ok()).unwrap_or(9185);
	let db_path = args.get_value('f').unwrap_or(String::from("/var/mail/outbound_queue.db"));
	//24 hours
	let retry_window_string = args.get_value('r').and_then(|w| w.parse().ok()).unwrap_or(60*60*24);
	let retry_window = Duration::new(retry_window_string,0);
	//====== setup email queue ======
	let raw_queue = match EmailQueue::new(db_path){
		Ok(q) => q,
		Err(e) => {
			eprintln!("Error initialising email queue database: {e}");
			return ExitCode::FAILURE;
		}
	};
	//====== choose whether to receive or send emails ======
	let mode = args.others().into_iter().map(|o| o.as_str()).next();
	match mode {
		Some("listen") => relay_recv(raw_queue,port),
		Some("send") => relay_send(raw_queue,retry_window),
		Some(_) => {
			eprintln!("Unrecognised mode.");
			ExitCode::FAILURE
		}
		None => {
			eprintln!("Please specify mode.");
			ExitCode::FAILURE
		}
	}
}

fn print_help(){
	use std::env;
	let name = env::args().next().unwrap_or("smtprelay".into());
	println!("Usage: {name} <listen|send> [options]");
	println!("Operates in 2 modes. listen accepts emails and send relays them to their destination.");
	println!("Requires both for full relay functionality (spawn as seperate processes)");
	println!("Options:");
	println!("	-h, --help         : Print this text");
	println!("	-p, --port         : Port to listen on. Only works in listen mode");
	println!("	-f, --db-path      : Path for the queue database. Seperate db to the smtpserver.");
	println!("	-r, --retry-window : The number of seconds to keep attempting to send an email for.");

}
fn relay_send(queue: EmailQueue, retry_window: Duration) -> ExitCode {
	loop {
		//====== attempt to send next email in queue ======
		let queued_email = match queue.peek(){
			Ok(Some(email)) => email,
			Ok(None) => {
				//wait between checking if new emails have arived
				thread::sleep(Duration::new(10,0));
				continue;
			}
			Err(e) => {
				eprintln!("Error fetching email from queue: {e}");
				return ExitCode::FAILURE;
			}
		};
		let email = queued_email.email();
		println!("======> sending email to {:?}",email.recipients_vec());
		//send it
		let result = resolve_and_send_email(email);
		match result {
			Ok(_) => {
				//====== successfuly sent ======
				if let Err(e) = queue.delete(queued_email){
					eprintln!("Error deleting queued email: {e}");
				}
				println!("email sent");
			},
			Err(FailureType::Permanent(err)) => {
				//====== permanent failure ======
				eprintln!("Permanent failure sending email: {err}");
				if let Err(e) = queue.delete(queued_email){
					eprintln!("Error deleting queued email: {e}");
				}
			},
			Err(FailureType::Temporary(err)) => {
				//====== temporary failure (try again later) ======
				eprintln!("Temporary failure sending email: {err}");
				//stop retrying after a certain amount of time has passed
				if SystemTime::now() > queued_email.time_queued() + retry_window {
					eprintln!("Email past retry window: discarding");
					if let Err(e) = queue.delete(queued_email){
						eprintln!("Error deleting queued email: {e}");
					}
				}else if let Err(e) = queue.retry_later(queued_email){
					eprintln!("Error postponing queued email: {e}");
				};
			},
		};
	}
}

fn resolve_and_send_email(email: &Email) -> Result<(),FailureType<Box<dyn Error>>> {
	//====== send email to each recipient ======
	//the same email is split into seperate items in the queue for each recipient
	//so it is guaranteed to only have one recipient
	let Some(recipient) = email.recipients_vec().pop()
	else { return Ok(()) }; //no recipients means nothing to do
	//====== query mx record for recipient ======
	let mut mx_records = fetch_email_mx_records(&recipient)
		.map_err(|e| FailureType::Permanent(e))?;
	//use highest priority mx record
	let Some(mx_record) = mx_records.pop()
	else {
		return Err(FailureType::Permanent(Box::new(IoError::other("domain has no mx records"))));
	};
	//====== connect to recipient relay ======
	println!("==> attempting tls connection");
	match tls_connect(&mx_record,465){
		Ok(mut stream) => {
			//try tls first
			send_emails(&mut stream,vec![email.clone()])
				.map_err(|e| FailureType::Temporary(e))?;
			println!("==> sending emails...");
		}
		Err(e) => {
			eprintln!("tls error: {e}");
			//then fallback to plaintext
			println!("==> falling back to plaintext");
			let mut stream = TcpStream::connect((mx_record.clone(),25))
				.map_err(|e| FailureType::Temporary(e.into()))?;
			println!("==> sending emails...");
			send_emails(&mut stream,vec![email.clone()])
				.map_err(|e| FailureType::Temporary(e))?;
		}
	};
	Ok(())
}

fn tls_connect(destination: &str, port: u16) -> Result<StreamOwned<ClientConnection,TcpStream>,Box<dyn Error>> {
	let root_store = RootCertStore {
		roots: webpki_roots::TLS_SERVER_ROOTS.into(),
	};
	let config = ClientConfig::builder()
		.with_root_certificates(root_store)
		.with_no_client_auth();
	let name = ServerName::try_from(destination.to_string())?;
	let tls = ClientConnection::new(config.into(),name)?;
	println!("connecting...");
	let connection = TcpStream::connect_timeout(
		&(destination,port).to_socket_addrs()?.next().ok_or(IoError::other("No address could be resolved"))?,
		Duration::new(1,0)
	)?;
	println!("connected");
	Ok(StreamOwned::new(tls,connection))
}

fn relay_recv(queue: EmailQueue, port: u16) -> ExitCode {
	//====== listen for connections ======
	let listener = match TcpListener::bind(("0.0.0.0",port)) {
		Ok(l) => l, Err(e) => {
			eprintln!("failed to bind to port {port}: {e}");
			return ExitCode::FAILURE;
		}
	};
	println!("listening on port {port}...");
	loop {
		//====== process connection ======
		//ignore connection errors
		let Ok((connection,address)) = listener.accept() else {continue};
		println!("===> new outbound mail connection: {address}");
		let emails = match recieve_emails(connection) {
			Ok(emails) => emails,
			Err(err) => {
				eprintln!("error while receiving emails: {err}");
				continue;
			}
		};
		//====== queue new emails ======
		let mut errors = false;
		for email in emails {
			match queue.enqueue(email){
				Ok(_) => (),
				Err(e) => {
					eprintln!("Error enqueueing email: {e}");
					errors = true;
					continue;
				}
			}
		}
		if errors != true {println!("mail successfully queued");}
	}
}

fn fetch_email_mx_records(email_address: &str) -> Result<Vec<String>,Box<dyn Error>> {
	let domain: Name<Vec<u8>> = Name::from_str(email_address
		.split_once("@")
		.ok_or(io::Error::other("Invalid email address"))
		.map(|(_,d)| d)?)?;
	let domain_clone = domain.clone(); //domain moved into closure but also required for printing errors
	let result = StubResolver::run(
		move |resolver: StubResolver| async move {
			resolver.query((domain_clone,Rtype::MX)).await
		}
	)?;
	let mut records: Vec<_> = result
		.answer()?
		//extract pure mx records
		.limit_to::<Mx<_>>()
		.filter_map(|r| if let Ok(mx) = r {Some(mx.into_data())} else {None})
		//map to tuple (priority,exchange)
		.map(|r| (r.preference(),r.exchange().to_string()))
		.collect();
	//order by priority (low priority number = lower index)
	records.sort_by(|(priority1,_),(priority2,_)| priority2.cmp(priority1));
	//discard the priority
	Ok(records
		.into_iter()
		.map(|(_,exchange)| exchange)
		.collect())
}

