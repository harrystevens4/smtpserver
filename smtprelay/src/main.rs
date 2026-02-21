use maildb::Email;
mod emailqueue;

use crate::emailqueue::EmailQueue;
use smtp::{send_emails,recieve_emails};

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
	//====== setup email queue ======
	let email_queue = Arc::new(Mutex::new(EmailQueue::new()));
	let email_queue_copy = email_queue.clone();
	let processing_thread = thread::spawn(move || process_queue(email_queue_copy));
//	let mut test_email = Email::default();
//	test_email.data = 
//"From: \"harry\" <harry@stevens-server.co.uk>\r\n\
//To: \"harry\" <derrickotron5000@gmail.com>\r\n\
//Message-id: <YOCHAT>\r\n\
//Subject: Yo chat\r\n\
//\r\n\
//hello chat\
//".into();
//	test_email.senders = vec!["harry@stevens-server.co.uk".into()];
//	test_email.recipients = vec!["derrickotron5000@gmail.com".into()];
//	{
//		let mut queue = email_queue.lock().unwrap();
//		queue.enqueue(test_email.into());
//	}
	//====== listen for connections ======
	let port = 9185;
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
		{//<<< queue acquired >>>
			let mut queue = email_queue.lock().unwrap();
			for email in emails {
				queue.enqueue(email.into());
			}
		}//<<< queue released >>>
		println!("mail successfully queued");
		//====== check on email processing thread ======
		if processing_thread.is_finished(){
			panic!("email processing thread terminated unexpectedly");
		}
	}
}

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
