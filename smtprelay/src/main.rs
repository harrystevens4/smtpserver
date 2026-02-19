use maildb::Email;
use smtp::{send_emails};
use std::net::{TcpStream};
use std::process::ExitCode;

fn main() -> ExitCode {
	let mut test_email = Email::default();
	
	test_email.data = "From: harry a@b.com\r\nSubject: Awesomeness\r\nTo: harry derrickotron5000@gmail.com\r\nMessage-ID: <AAAAAAAAAA@stevens-server.co.uk>\r\n\r\nHi, whats up?".into();
	test_email.senders = vec![
		"thomas@stevens-server.co.uk",
		"harry@stevens-server.co.uk"
	].into_iter().map(String::from).collect();
	test_email.recipients = vec![
		"derrickotron5000@gmail.com"
	].into_iter().map(String::from).collect();

	let mut connection = match TcpStream::connect("gmail-smtp-in.l.google.com:25"){
		Ok(c) => c, Err(e) => {
			eprintln!("Error connecting: {e}");
			return ExitCode::FAILURE;
		}
	};
	match send_emails(&mut connection,vec![test_email]){
		Ok(_) => ExitCode::SUCCESS,
		Err(e) => {
			eprintln!("Error sending emails: {e}");
			ExitCode::FAILURE
		}
	}
}
