use std::{io,error};
use std::net::TcpStream;
use std::io::{Read,Write};
use std::io::ErrorKind;
use maildb::{MailDB,Email};
use std::error::Error;
use std::default::Default;
use std::any::Any;
use std::time::Duration;

use rustls::{StreamOwned,ServerConfig,ServerConnection};
use rustls_pki_types::{CertificateDer,PrivateKeyDer};
use rustls_pki_types::pem::PemObject;

use sha256;

pub trait ReadWrite: Read + Write + Any {}
impl ReadWrite for TcpStream {}
impl ReadWrite for StreamOwned<ServerConnection,TcpStream> {}

pub struct POP3Config {
	pub tls_enabled: bool,
	pub tls_private_key: Option<String>,
	pub tls_certs: Option<String>,
	timeout: Duration,
}
impl Default for POP3Config {
	fn default() -> Self {
		POP3Config {
			tls_enabled: false,
			tls_private_key: None,
			tls_certs: None,
			timeout: Duration::new(5,0), //5 seconds
		}
	}
}
impl POP3Config {
	pub fn timeout(&self) -> Duration {self.timeout.clone()}
	pub fn set_timeout(&mut self, timeout: Duration) {self.timeout = timeout}
}

pub fn pop3_handshake(connection: &mut TcpStream) -> io::Result<()> {
	connection.write(b"+OK ready\r\n")?;
	Ok(())
}


pub fn pop3_authenticate<
	U: Fn(&str) -> Result<bool,Box<dyn error::Error>>,
	P: Fn(&str,&str) -> Result<bool,Box<dyn error::Error>>
>(stream: TcpStream, pop3_config: &POP3Config, verify_user: U, verify_pass: P) -> Result<(String,Box<dyn ReadWrite>),Box<dyn error::Error>> {
	let mut connection = Box::new(stream) as Box<dyn ReadWrite>;
	loop {
		let line = readline(&mut connection)?;
		let mut split_line = line.split(' ');
		if let Some(command) = split_line.next(){
			match command.to_ascii_uppercase().as_str(){
				"CAPA" => {
					connection.write(b"+OK list follows\r\n")?;
					connection.write(b"USER\r\n")?;
					if pop3_config.tls_enabled {
						connection.write(b"STLS\r\n")?;
					}
					connection.write(b".\r\n")?;
				},
				"STLS" => {
					//====== check for tls support ======
					if !pop3_config.tls_enabled {
						connection.write(b"-ERR tls not supported\r\n")?;
						continue;
					}
					match (connection as Box<dyn Any>).downcast::<TcpStream>(){
						Ok(mut tcp_stream) => {
							tcp_stream.write(b"+OK Begin negotiations\r\n")?;
							println!("attempting tls upgrade...");
							//====== try to upgrade ======
							connection = match tls_upgrade((*tcp_stream).try_clone()?,pop3_config){
								Ok(stream) => Box::new(stream) as Box<dyn ReadWrite>,
								Err(error) => {
									eprintln!("Error upgrading TLS: {error}");
									tcp_stream
								}
							};
						},
						Err(tcp_stream) => {
							//====== already using tls ======
							//put the old connection back
							connection = tcp_stream
								.downcast::<TcpStream>()
								.map_err(|_| io::Error::other("Box downcast failed"))?
								as Box<dyn ReadWrite>;
							connection.write(b"-ERR tls already active\r\n")?;
							continue;
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
					let mut maildrop_len = 0;
					for email in &maildrop {
						maildrop_len += email.data().len();
					}
					let maildrop = format!("+OK {} {}\r\n",maildrop.len(),maildrop_len);
					connection.write(&maildrop.into_bytes())?;
				},
				"NOOP" => {
					connection.write(b"+OK\r\n")?;
				}
				"UIDL" => {
					if let Some(arg) = split_line.next(){
						//specific mail
						let Ok(mail_id) = arg.parse::<usize>() else {
							connection.write(b"-ERR Could not parse\r\n")?;
							continue;
						};
						if let Some(email) = mail_id.checked_sub(1).and_then(|i| maildrop.get(i)){
							let email: &Email = email; //type annotations required
							let unique_id = sha256::digest(
								email.data() + &email.timestamp().to_string()
							)[..20].to_string();
							let listing = format!("+OK {} {}\r\n",mail_id,unique_id);
							connection.write(&listing.into_bytes())?;
						}else {
							connection.write(b"-ERR Bad mail id\r\n")?;
							continue;
						}
					}else{
						//all mail
						connection.write(b"+OK\r\n")?;
						for (i,email) in maildrop.iter().enumerate(){
							let unique_id = sha256::digest(
								email.data() + &email.timestamp().to_string()
							)[..20].to_string();
							let listing = format!("{} {}\r\n",i+1,unique_id);
							connection.write(&listing.into_bytes())?;
						}
						connection.write(b".\r\n")?;
					}
				},
				"LIST" => {
					if let Some(arg) = split_line.next(){
						//specific mail
						let Ok(mail_id) = arg.parse::<usize>() else {
							connection.write(b"-ERR Could not parse\r\n")?;
							continue;
						};
						if let Some(email) = mail_id.checked_sub(1).and_then(|i| maildrop.get(i)) {
							let listing = format!("+OK {} {}\r\n",mail_id,email.data().len());
							connection.write(&listing.into_bytes())?;
						}else {
							connection.write(b"-ERR Bad mail id\r\n")?;
							continue;
						}
					}else{
						//all mail
						connection.write(b"+OK\r\n")?;
						for (i,email) in maildrop.iter().enumerate() {
							let message_length = email.data().len();
							let listing = format!("{} {}\r\n",i+1,message_length);
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
					let Ok(mail_id) = arg.parse::<usize>() else {
						connection.write(b"-ERR Could not parse\r\n")?;
						continue;
					};
					//actualy fetch it
					if let Some(email) = mail_id.checked_sub(1).and_then(|i| maildrop.get(i)) {
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
					let Ok(mail_id) = arg.parse::<usize>() else {
						connection.write(b"-ERR Could not parse\r\n")?;
						continue;
					};
					//fetch database mail id
					let Some(email) = mail_id.checked_sub(1).and_then(|i| maildrop.get(i))
					else {
						connection.write(b"-ERR Invalid mail id\r\n")?;
						continue;
					};
					emails_to_delete.push(email.id());
					connection.write(b"+OK\r\n")?;
				},
				"RSET" => {
					emails_to_delete.clear();
					connection.write(b"+OK\r\n")?;
				},
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
	let Some(ref certs_file) = config.tls_certs
		else {Err(io::Error::other("no tls certificate provided"))?};
	let Some(ref private_key_file) = config.tls_private_key
		else {Err(io::Error::other("no tls private key provided"))?};
	let certs = CertificateDer::pem_file_iter(certs_file)?
		.filter_map(|c| c.ok())
		.collect();
	let private_key = PrivateKeyDer::from_pem_file(private_key_file)?;
	//====== build the config ======
	let config = ServerConfig::builder()
		.with_no_client_auth()
		.with_single_cert(certs,private_key)?;
	//return final stream
	Ok(StreamOwned::new(ServerConnection::new(config.into())?,connection))
}
