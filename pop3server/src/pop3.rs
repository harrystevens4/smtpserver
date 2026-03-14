use std::{io,error};
use std::net::TcpStream;
use std::io::{Read,Write};
use std::io::ErrorKind;
use maildb::MailDB;
use std::error::Error;
use std::default::Default;
use std::any::Any;

use rustls::{StreamOwned,ServerConfig,ServerConnection};
use rustls_pki_types::{CertificateDer,PrivateKeyDer};

pub trait ReadWrite: Read + Write + Any {}
impl ReadWrite for TcpStream {}
impl ReadWrite for StreamOwned<ServerConnection,TcpStream> {}

#[derive(Default)]
pub struct POP3Config {
	tls_enabled: bool,
	tls_private_key: Option<PrivateKeyDer<'static>>,
	tls_certs: Option<Vec<CertificateDer<'static>>>,
	domain: String,
}

pub fn pop3_handshake(connection: &mut TcpStream) -> io::Result<()> {
	connection.write(b"+OK ready\r\n")?;
	Ok(())
}


pub fn pop3_authenticate<
	U: Fn(&str) -> Result<bool,Box<dyn error::Error>>,
	P: Fn(&str,&str) -> Result<bool,Box<dyn error::Error>>
>(mut stream: TcpStream, verify_user: U, verify_pass: P) -> Result<(String,Box<dyn ReadWrite>),Box<dyn error::Error>> {
	let mut tls_active = false;
	let mut connection = Box::new(stream) as Box<dyn ReadWrite>;
	loop {
		let line = readline(&mut connection)?;
		let mut split_line = line.split(' ');
		if let Some(command) = split_line.next(){
			match command.to_ascii_uppercase().as_str(){
				"CAPA" => {
					connection.write(b"+OK list follows\r\n")?;
					connection.write(b"USER\r\n")?;
					connection.write(b"STLS\r\n")?;
					connection.write(b".\r\n")?;
				},
				"STLS" => {
					if tls_active == true {
						connection.write(b"-ERR tls already active\r\n")?;
						continue;
					}
					tls_active = true;
					connection.write(b"+OK Begin negotiations\r\n")?;
					let tcp_stream: Box<TcpStream> = (connection as Box<dyn Any>)
						.downcast()
						.map_err(|_| io::Error::other("Could not downcast Box"))?;

					connection = match tls_upgrade((*tcp_stream).try_clone()?,&POP3Config::default()){
						Ok(stream) => Box::new(stream) as Box<dyn ReadWrite>,
						Err(error) => {
							eprintln!("Error upgrading TLS: {error}");
							tcp_stream
						}
					}
				}
				"USER" => {
					let user = split_line.next();
					//verify user
					if user.is_none() || !verify_user(&user.unwrap())?{
						connection.write(b"-ERR Bad user\r\n")?;
						continue;
					}
					//fetch password
					connection.write(b"+OK\r\n")?;
					let line = readline(&mut connection)?;
					let mut split_line = line.split(' ');
					if split_line.next().map(|s| s.to_ascii_uppercase()) == Some("PASS".to_string()) {
						if let Some(password) = split_line.next() && verify_pass(&user.unwrap(),password)?{
							//verify password
							connection.write(b"+OK\r\n")?;
							return Ok((
								user
									.ok_or(io::Error::other("User undefined"))
									.map(String::from)?,
								connection
							));
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

pub fn pop3_process_transactions(connection: &mut dyn ReadWrite, mail_db: &MailDB, user: &str) -> Result<(),Box<dyn error::Error>> {
	//make an in memory copy of the user's mail
	let maildrop = mail_db.retrieve_mail(user)?;
	let mut emails_to_delete: Vec<usize> = vec![];
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
						if let Some(email) = maildrop.iter().find(|m| m.id() == mail_id){
							let listing = format!("+OK {} {}\r\n",email.id(),email.data().len());
							connection.write(&listing.into_bytes())?;
						}else {
							connection.write(b"-ERR Bad mail id\r\n")?;
							continue;
						}
					}else{
						//all mail
						connection.write(b"+OK\r\n")?;
						for email in &maildrop {
							let message_length = email.data().len();
							let listing = format!("{} {}\r\n",email.id(),message_length);
							connection.write(&listing.into_bytes())?;
						}
						connection.write(b".\r\n")?;
					}
				},
				"RETR" => {
					//get mail to retrieve
					let Some(arg) = split_line.next() else {
						connection.write(b"-ERR No argument provided\r\n")?;
						continue;
					};
					let Ok(mail_id) = arg.parse() else {
						connection.write(b"-ERR Could not parse\r\n")?;
						continue;
					};
					//actualy fetch it
					if let Some(email) = maildrop.iter().find(|m| m.id() == mail_id){
						let listing = format!("+OK\r\n");
						connection.write(&listing.into_bytes())?;
						//mail is stored without trailing CRLF
						let data = email.data() + "\r\n";
						connection.write(&data.into_bytes())?;
						connection.write(b".\r\n")?;

					}else {
						connection.write(b"-ERR Bad mail id\r\n")?;
						continue;
					}
				},
				"DELE" => {
					let Some(arg) = split_line.next() else {
						connection.write(b"-ERR No argument provided\r\n")?;
						continue;
					};
					let Ok(mail_id) = arg.parse() else {
						connection.write(b"-ERR Could not parse\r\n")?;
						continue;
					};
					emails_to_delete.push(mail_id);
					connection.write(b"+OK\r\n")?;
				},
				"RSET" => {
					emails_to_delete.clear();
					connection.write(b"+OK\r\n")?;
				}
				"QUIT" => {
					//move to UPDATE state
					//commit all the deleted messages
					let result = emails_to_delete
						.into_iter()
						.try_for_each(|id| mail_db.delete_email(id));
					if result.is_ok(){
						connection.write(b"+OK\r\n")?;
					}else {
						connection.write(b"+ERR failed to delete some emails\r\n")?;
						result?;
					}
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

fn readline(stream: &mut dyn Read) -> io::Result<String> {
	let mut line_buffer: Vec<u8> = vec![];
	loop {
		let mut read_buffer = [0_u8; 1];
		let bytes_read = stream.read(&mut read_buffer)?;
		if bytes_read == 0 {return Err(io::Error::from(ErrorKind::ConnectionReset))}
		else {
			line_buffer.extend_from_slice(&read_buffer);
		}
		let line_len = line_buffer.len();
		if line_buffer.len() > 0 && &line_buffer[line_len-1..] == b"\n" {break}
	}
	//adjust line length to omit trailing "\n" or "\r\n" if present
	let line_length = if line_buffer.len() > 1 && &line_buffer[line_buffer.len()-2..] == b"\r\n" {
		line_buffer.len() - 2
	}else {
		line_buffer.len() - 1
	};
	//final buffer w/o \n
	Ok(line_buffer
		.into_iter()
		.map(char::from)
		.take(line_length)//strip training \r\n
		.collect::<String>()
		.into()
	)
}

fn tls_upgrade(connection: TcpStream, config: &POP3Config) -> Result<StreamOwned<ServerConnection,TcpStream>,Box<dyn Error>> {
	//====== verify certificates and private key present ======
	let Some(ref certs) = config.tls_certs
		else {Err(io::Error::other("no tls certificate provided"))?};
	let Some(ref private_key) = config.tls_private_key
		else {Err(io::Error::other("no tls private key provided"))?};
	//====== build the config ======
	let config = ServerConfig::builder()
		.with_no_client_auth()
		.with_single_cert(certs,private_key)?;
	//return final stream
	Ok(StreamOwned::new(ServerConnection::new(config.into())?,connection))
}
