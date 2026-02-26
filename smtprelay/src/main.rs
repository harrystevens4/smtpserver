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
		Some("send") => ExitCode::SUCCESS,
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
		for email in emails {
			match queue.enqueue(email){
				Ok(_) => (),
				Err(e) => {
					eprintln!("Error enqueueing email: {e}");
					continue;
				}
			}
		}
		println!("mail successfully queued");
	}
}

/*
fn process_queue(queue: Arc<Mutex<EmailQueue>>){
	loop {
		//====== acquire lock on queue ======
		let mut processing_queue = EmailQueue::new();
		{//<<< queue aquisition >>>
			let mut queue = queue.lock().unwrap();
			for _ in 0..((*queue).len()){
				processing_queue.enqueue((*queue).dequeue())
			}
		}//<<< queue relinquished >>>
		//====== attempt to send every email in the queue ======
		for _ in 0..((processing_queue).len()){
			let queued_email = processing_queue.dequeue();
			println!("sending email {queued_email:?}");
			let email = queued_email.email();
			//====== send email to each recipient ======
			for recipient in email.recipients_vec(){
				//====== query mx record for recipient ======
				let mut mx_records = match fetch_email_mx_records(&recipient){
					Ok(r) => r, Err(e) => {
						eprintln!("Error fetching mx records for {recipient}: {e}");
						continue;
					}
				};
				//use highest priority mx record
				let Some(mx_record) = mx_records.pop() else {
					eprintln!("No mx records found for domain {recipient}");
					continue;
				};
				//====== connect to recipient relay ======
				let mut connection = match TcpStream::connect((mx_record,25)){
					Ok(c) => c, Err(e) => {
						eprintln!("Error connecting: {e}");
						eprintln!("reattempting later");
						continue;
					}
				};
				match send_emails(&mut connection,vec![email.clone()]){
					Ok(_) => (),
					Err(e) => eprintln!("Error sending emails: {e}"),
				}
			}
		}
		//wait 20 seconds between rounds of sending emails
		thread::sleep(Duration::new(20,0));
	}
}
*/

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
