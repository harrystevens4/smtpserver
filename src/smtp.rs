use crate::email::Email;
use std::net::TcpStream;
use std::error::Error;
use std::io::{Read,Write,ErrorKind};
use std::io;

pub fn recieve_emails(mut connection: TcpStream) -> Result<Vec<Email>,Box<dyn Error>>{
	//====== handshake ======
	smtp_handshake(&mut connection)?;
	//====== process mail ======
	let mut emails = vec![];
	loop {
		let email = match smtp_receive_email(&mut connection){
			//no more emails
			Err(err) if err.kind() == ErrorKind::ConnectionReset => {break},
			//error
			Err(e) => return Err(Box::new(e)),
			//successful receipt of new email
			Ok(email) => email,
		};
		emails.push(email);
	}
	Ok(emails)
}

fn smtp_handshake(connection: &mut TcpStream) -> io::Result<()>{
	//ack connection
	let _ = connection.write(b"220 smtpserver\r\n");
	//wait for greeting
	let buffer = readline(connection)?;
	//verify greeting
	if !buffer.to_ascii_uppercase().starts_with("HELO"){
		return Err(io::Error::other("malformed greeting in request"));
	}
	Ok(())
}

fn smtp_receive_email(connection: &mut TcpStream) -> io::Result<Email>{
	let mut senders: Vec<String> = vec![];
	let mut recipients: Vec<String> = vec![];
	loop {
		let line = readline(connection)?;
		if line.to_ascii_uppercase().starts_with("QUIT"){
			//====== end of mail ======
			return Err(io::Error::from(io::ErrorKind::ConnectionReset));
		}else if line.to_ascii_uppercase().starts_with("MAIL FROM"){
			//====== senders ======
			let sender = line.split_once(':').ok_or(io::Error::other("bad smtp command"))?.1
 				// extract address from between < and > brackets
				.split_once('<').ok_or(io::Error::other("bad smtp address"))?.1
				.split_once('>').ok_or(io::Error::other("bad smtp address"))?.0;
			senders.push(sender.to_string());
			//send positive ack
			connection.write(b"250 Ok");
		}else if line.to_ascii_uppercase().starts_with("RCPT TO"){
			//====== recipients ======
			let recipient = line.split_once(':').ok_or(io::Error::other("bad smtp command"))?.1
 				// extract address from between < and > brackets
				.split_once('<').ok_or(io::Error::other("bad smtp address"))?.1
				.split_once('>').ok_or(io::Error::other("bad smtp address"))?.0;
			recipients.push(recipient.to_string());
			//send positive ack
			connection.write(b"250 Ok");
		}else if line.to_ascii_uppercase().starts_with("RCPT TO"){
			//====== email body ======
		}else {
			//====== command error ======
			connection.write(b"500 Unknown command");
			continue;
		}
	}
	Ok(Email::default())
}

fn readline(stream: &mut TcpStream) -> io::Result<String> {
	let mut line_buffer: Vec<u8> = vec![];
	loop {
		let mut read_buffer = [0; 256];
		stream.peek(&mut read_buffer)?;
		if let Some(line_length) = read_buffer
			.iter()
			.map(|c| char::from(*c))
			.collect::<String>()
			.find('\n'){

		}else {
			stream.read(&mut read_buffer)?;
			line_buffer.extend_from_slice(&read_buffer);
		}
	}
	//final buffer w/o \n
	Ok(line_buffer
		.into_iter()
		.map(char::from)
		.collect::<String>()
		.trim_end_matches('\r')
		.into()
	)
}
