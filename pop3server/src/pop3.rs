use std::{io,error};
use std::net::TcpStream;
use std::io::{Read,Write};
use std::io::ErrorKind;
use maildb::MailDB;

pub fn pop3_handshake(connection: &mut TcpStream) -> io::Result<()> {
	connection.write(b"+OK ready\r\n")?;
	Ok(())
}


pub fn pop3_authenticate<
	U: Fn(&str) -> Result<bool,Box<dyn error::Error>>,
	P: Fn(&str,&str) -> Result<bool,Box<dyn error::Error>>
>(connection: &mut TcpStream, verify_user: U, verify_pass: P) -> Result<String,Box<dyn error::Error>> {
	loop {
		let line = readline(connection)?;
		let mut split_line = line.split(' ');
		if let Some(command) = split_line.next(){
			match command.to_ascii_uppercase().as_str(){
				"CAPA" => {
					connection.write(b"+OK list follows\r\n")?;
					connection.write(b"USER\r\n")?;
					connection.write(b".\r\n")?;
				},
				"USER" => {
					let user = split_line.next();
					//verify user
					if user.is_none() || !verify_user(&user.unwrap())?{
						connection.write(b"-ERR Bad user\r\n")?;
						continue;
					}
					//fetch password
					connection.write(b"+OK\r\n")?;
					let line = readline(connection)?;
					let mut split_line = line.split(' ');
					if split_line.next().map(|s| s.to_ascii_uppercase()) == Some("PASS".to_string()) {
						if let Some(password) = split_line.next() && verify_pass(&user.unwrap(),password)?{
							//verify password
							connection.write(b"+OK\r\n")?;
							return Ok(user
								.ok_or(io::Error::other("User undefined"))
								.map(String::from)?);
						}
					}
					connection.write(b"-ERR Bad password\r\n")?;
				}
				"QUIT" => {
					connection.write(b"+OK\r\n")?;
					return Err(io::Error::from(ErrorKind::ConnectionReset))?;
				}
				_ => {
					connection.write(b"+ERR Unknown command\r\n")?;
				}
			}
		}
	}
}

pub fn pop3_process_transactions(connection: &mut TcpStream, mail_db: &MailDB, user: &str) -> Result<(),Box<dyn error::Error>> {
	//make an in memory copy of the user's mail
	let maildrop = mail_db.retrieve_mail(user)?;
	loop {
		let line = dbg!{readline(connection)?};
		let mut split_line = line.split(' ');
		if let Some(command) = split_line.next(){
			match command.to_ascii_uppercase().as_str(){
				"STAT" => {
					let maildrop = format!("+OK {} {}\r\n",maildrop.len(),1024);
					connection.write(&maildrop.into_bytes())?;
				},
				"NOOP" => {
					connection.write(b"+OK\r\n")?;
				}
				"UIDL" | "LIST" => {
					if let Some(arg) = split_line.next(){
						//specific mail
						let Ok(mail_id) = arg.parse() else {
							connection.write(b"-ERR Could not parse\r\n")?;
							continue;
						};
						if let Some(email) = maildrop.iter().find(|m| m.id == mail_id){
							let listing = format!("+OK {} {}\r\n",email.id,email.data().len());
							connection.write(&listing.into_bytes())?;
						}else {
							connection.write(b"-ERR Bad mail id\r\n")?;
							continue;
						}
					}else{
						//all mail
						connection.write(b"+OK\r\n")?;
						for email in &maildrop {
							let message_length = email.data.len();
							let listing = format!("{} {}\r\n",email.id,message_length);
							connection.write(&listing.into_bytes())?;
						}
						connection.write(b".\r\n")?;
					}
				},
				"RETR" => {
					let Some(arg) = split_line.next() else {
						connection.write(b"-ERR No argument provided\r\n")?;
						continue;
					};
					let Ok(mail_id) = arg.parse() else {
						connection.write(b"-ERR Could not parse\r\n")?;
						continue;
					};
					if let Some(email) = maildrop.iter().find(|m| m.id == mail_id){
						let listing = format!("+OK\r\n");
						connection.write(&listing.into_bytes())?;
						let data = email.data() + "\r\n";
						connection.write(&data.into_bytes())?;
						connection.write(b".\r\n")?;

					}else {
						connection.write(b"-ERR Bad mail id\r\n")?;
						continue;
					}
				},
				"QUIT" => {
					connection.write(b"+Ok\r\n")?;
					return Ok(());
				},
				_ => {
					connection.write(b"-ERR Unknown command\r\n")?;
					continue
				}
			}
		}
	}
}

fn readline(stream: &mut TcpStream) -> io::Result<String> {
	let mut line_buffer: Vec<u8> = vec![];
	loop {
		let mut read_buffer = [0; 256];
		let bytes_read = stream.peek(&mut read_buffer)?;
		if bytes_read == 0 {return Err(io::Error::from(ErrorKind::ConnectionReset))}
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
