use maildb::Email;
mod emailqueue;

use crate::emailqueue::EmailQueue;
use smtp::{send_emails,recieve_emails};
use args::Args;

use std::str::FromStr;
use std::io;
use std::net::{TcpStream,TcpListener};
use std::process::ExitCode;
use std::sync::{Arc,Mutex};
use std::thread;
use std::time::{Duration};
use std::error::Error;
use domain::resolv::stub::StubResolver;
use domain::base::iana::{Rtype};
use domain::base::name::Name;
use domain::rdata::rfc1035::Mx;

fn main() -> ExitCode {
	//====== process command line arguments ======
	let args = Args::gather(&[
		('h', Some("help"),    false ),
		('p', Some("port"),    true  ),
		('f', Some("db-path"), true  ),
	]);
	let port = args.get_value('p').and_then(|p| p.parse().ok()).unwrap_or(9185);
	let db_path = args.get_value('f').unwrap_or(String::from("queue.db"));
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
		Some("send") => relay_send(raw_queue),
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

fn relay_send(queue: EmailQueue) -> ExitCode {
	loop {
		//====== attempt to send next email in queue ======
		let queued_email = match queue.peek(){
			Ok(Some(email)) => email,
			Ok(None) => {
				//wait 1 second between checking
				thread::sleep(Duration::new(1,0));
				continue;
			}
			Err(e) => {
				eprintln!("Error fetching email from queue: {e}");
				return ExitCode::FAILURE;
			}
		};
		let email = queued_email.email();
		println!("sending email to {:?}",email.recipients_vec());
		//====== send email to each recipient ======
		//the same email is split into seperate items in the queue for each recipient
		//so it is guaranteed to only have one recipient
		let Some(recipient) = email.recipients_vec().pop()
		else {
			eprintln!("email has no recipients - discarding");
			if let Err(e) = queue.delete(queued_email){
				eprintln!("Error deleting queued email: {e}")
			};
			continue;
		};
		//====== query mx record for recipient ======
		let mut mx_records = match fetch_email_mx_records(&recipient){
			Ok(r) => r, Err(e) => {
				eprintln!("Error fetching mx records for {recipient} - discarding: {e}");
				//permanent failure
				if let Err(e) = queue.delete(queued_email){
					eprintln!("Error deleting queued email: {e}");
				};
				continue;
			}
		};
		//use highest priority mx record
		let Some(mx_record) = mx_records.pop() else {
			eprintln!("No mx records found for domain {recipient} - discarding");
			//permanent failure
			if let Err(e) = queue.delete(queued_email){
				eprintln!("Error deleting queued email: {e}");
			};
			continue;
		};
		//====== connect to recipient relay ======
		let mut connection = match TcpStream::connect((mx_record,25)){
			Ok(c) => c, Err(e) => {
				eprintln!("Error connecting: {e} - postponing");
				//temporary failure
				if let Err(e) = queue.retry_later(queued_email){
					eprintln!("Error postponing queued email: {e}");
				};
				continue;
			}
		};
		match send_emails(&mut connection,vec![email.clone()]){
			Ok(_) => (),
			Err(e) => {
				eprintln!("Error sending emails: {e}");
				//temporary failure
				if let Err(e) = queue.retry_later(queued_email){
					eprintln!("Error postponing queued email: {e}");
				};
				continue;
			}
		}
		//successfuly sent
		if let Err(e) = queue.delete(queued_email){
			eprintln!("Error deleting queued email: {e}");
		}
		println!("email sent");
	}
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
