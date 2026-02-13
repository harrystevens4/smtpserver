use crate::email::Email;
use std::net::{TcpStream,Shutdown};
use std::error::Error;
use std::io::{Read,Write,ErrorKind};
use std::io;

pub fn recieve_emails(mut connection: TcpStream) -> Result<Vec<Email>,Box<dyn Error>>{
	//====== handshake ======
	smtp_handshake(&mut connection)?;
	//====== process mail ======
	let mut emails = vec![];
	//multiple messages, one connection
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
		//mail has been stored
		connection.write(b"250 Ok\r\n")?;
	}
	//close connection
	connection.shutdown(Shutdown::Both)?;
	Ok(emails)
}

fn smtp_handshake(connection: &mut TcpStream) -> io::Result<()>{
	//ack connection
	connection.write(b"220 smtpserver\r\n")?;
	//3 attempts to send a valid handshake
	for _ in 0..2 {
		//wait for greeting
		let buffer = readline(connection)?;
		//verify greeting
		if buffer.to_ascii_uppercase().starts_with("HELO"){
			connection.write(b"250 Ok\r\n")?;
			return Ok(());
		}else {
			connection.write(b"502 Unsupported\r\n")?;
		}
	}
	Err(io::Error::other("malformed greeting in request"))
}

fn smtp_receive_email(connection: &mut TcpStream) -> io::Result<Email>{
	//=> based off RFC 5321 <=//
	let mut senders: Vec<String> = vec![];
	let mut recipients: Vec<String> = vec![];
	let mut body = String::new();
	loop {
		let line = readline(connection)?;
		if line.to_ascii_uppercase().starts_with("QUIT"){
			//====== end of mail ======
			return Err(io::Error::from(io::ErrorKind::ConnectionReset));
		}else if line.to_ascii_uppercase().starts_with("MAIL FROM"){
			//====== senders ======
			let Some(sender) = line.split_once(':')
 				// extract address from between < and > brackets
				.map(|(_,x)| x.split_once('<')).flatten()
				.map(|(_,x)| x.split_once('>')).flatten()
				.map(|(x,_)| x)
			else {
				connection.write(b"501 Syntax error\r\n")?;
				continue;
			};
			senders.push(sender.to_string());
			//send positive ack
			connection.write(b"250 Ok\r\n")?;
		}else if line.to_ascii_uppercase().starts_with("RCPT TO"){
			//====== recipients ======
			// extract address from between < and > brackets 
			let Some(recipient) = line.split_once(':')
				.map(|(_,x)| x.split_once('<')).flatten()
				.map(|(_,x)| x.split_once('>')).flatten()
				.map(|(x,_)| x)
			else {
				connection.write(b"501 Syntax error\r\n")?;
				continue;
			};
			recipients.push(recipient.to_string());
			//send positive ack
			connection.write(b"250 Ok\r\n")?;
		}else if line.to_ascii_uppercase().starts_with("DATA"){
			//====== email body ======
			//send intermediate reply
			connection.write(b"354 Ok\r\n")?;
			//receive all lines of the body
			loop {
				let body_line = readline(connection)?;
				//end of body
				if body_line == "." {break}
				//store the line
				body += &(body_line + "\n");
			}
			body = body.trim_end_matches("\n").to_string();
			//exit
			break;
		}else {
			//====== command error ======
			connection.write(b"500 Unknown command\r\n")?;
			continue;
		}
	}
	//====== construct the new email ======
	Ok(Email {
		senders,
		recipients,
		body,
	})
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
			.find('\n')
			.map(|n| n+1)
		{
			stream.read(&mut read_buffer[..line_length])?;
			line_buffer.extend_from_slice(&read_buffer[..line_length]);
			break;
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
		.trim_end_matches('\n')
		.trim_end_matches('\r')
		.into()
	)
}
