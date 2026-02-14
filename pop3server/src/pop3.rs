use std::io;
use std::net::TcpStream;
use std::io::{Read,Write};
use std::io::ErrorKind;

pub fn pop3_handshake(connection: &mut TcpStream) -> io::Result<()> {
	connection.write(b"+OK ready\r\n")?;
	Ok(())
}

pub fn pop3_authenticate<U: Fn(&str) -> bool, P: Fn(&str) -> bool>(connection: &mut TcpStream, verify_user: U, verify_pass: P) -> io::Result<()> {
	loop {
		let line = dbg!{readline(connection)}?;
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
					if user.is_none() || !verify_user(&user.unwrap()){
						connection.write(b"-ERR Bad user\r\n")?;
						continue;
					}
					//fetch password
					connection.write(b"+OK\r\n")?;
					let line = dbg!{readline(connection)?};
					let mut split_line = line.split(' ');
					if split_line.next().map(|s| s.to_ascii_uppercase()) == Some("USER".to_string()) {
						if let Some(password) = split_line.next() && verify_pass(password){
							//verify password
							connection.write(b"+OK\r\n")?;
							continue;
						}
					}
					connection.write(b"-ERR Bad password\r\n")?;
				}
				"QUIT" => {
					connection.write(b"+OK\r\n")?;
					return Err(io::Error::from(ErrorKind::ConnectionReset));
				}
				_ => {
					connection.write(b"+ERR Unknown command\r\n")?;
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
