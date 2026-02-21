use maildb::Email;
mod emailqueue;

use crate::emailqueue::EmailQueue;
use smtp::{send_emails};

use std::str::FromStr;
use std::net::{TcpStream};
use std::process::ExitCode;
use std::sync::{Arc,Mutex};
use std::thread;
use std::time::{Duration};
use domain::resolv::stub::StubResolver;
use domain::base::iana::{Rtype};
use domain::base::name::Name;

fn main() -> ExitCode {
	//====== setup email queue ======
	let email_queue = Arc::new(Mutex::new(EmailQueue::new()));
	let email_queue_copy = email_queue.clone();
	thread::spawn(move || process_queue(email_queue_copy));
	let mut test_email = Email::default();
	test_email.data = 
"From: \"harry\" <harry@stevens-server.co.uk?\r\n\
To: \"harry\" <harry@derrickotron5000@gmail.com>\r\n\
Message-id: <ABARACADABARA>\r\n\
Subject: Yo chat\r\n\
\r\n\
hello chat\
".into();
	test_email.senders = vec!["harry@stevens-server.co.uk".into()];
	test_email.recipients = vec!["harry@derrickotron5000@gmail.com".into()];
	{
		let mut queue = email_queue.lock().unwrap();
		queue.enqueue(test_email.into());
	}
	//====== listen for connections ======
	loop {
	}
}

fn process_queue(queue: Arc<Mutex<EmailQueue>>){
	loop {
		//====== acquire lock on queue ======
		{//<<< queue aquisition >>>
		let mut queue = queue.lock().unwrap();
		//====== attempt to send every email in the queue ======
		for _ in 0..((*queue).len()){
			let queued_email = queue.dequeue();
			let email = queued_email.email();
			//====== send email to each recipient ======
			for recipient in email.recipients_vec(){
				//====== query mx record for recipient ======
				println!("{mx_record:?}");
				let mut connection = match TcpStream::connect((mx_record,25)){
					Ok(c) => c, Err(e) => {
						eprintln!("Error connecting: {e}");
						continue;
					}
				};
				match send_emails(&mut connection,vec![email.clone()]){
					Ok(_) => (),
					Err(e) => eprintln!("Error sending emails: {e}"),
				}
			}
		}//<<< queue relinquished >>>
		}
		//wait 20 seconds between rounds of sending emails
		thread::sleep(Duration::new(20,0));
	}
}

fn fetch_email_mx_records(email_address: &str) -> Result<Vec<String>,Box<dyn Error>> {
	let Some(Ok(domain)): Option<Result<Name<Vec<u8>>,_>> = email_address
		.split_once("@")
		.map(|(_,d)| d)
		.map(|d: &str| Name::from_str(d))
	else{
		eprintln!("Error sending email to {recipient}: bad domain - ignoring");
		return Err()?
	};
	let domain_clone = domain.clone(); //domain moved into closure but also required for printing errors
	let result = StubResolver::run(
		move |resolver: StubResolver| async move {
			resolver.query((domain_clone,Rtype::MX)).await
		}
	);
	let mut mx_record = match result {
		Ok(answer) => String::from_utf8_lossy(answer
			.answer()
			.(|.next()
			.unwrap_or("")
		).into_owned(),
		Err(e) => {
			eprintln!("Error fetching mx record for {domain}: {e}");
			continue;
		}
	};
}
