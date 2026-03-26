use maildb::Email;
use std::time::Duration;
use std::net::{TcpStream};
use std::error::Error;
use std::io::{Read,Write,ErrorKind};
use std::io;
use std::default::Default;
use std::any::Any;
use std::mem;
//use std::time::{Duration};

use rustls::{ClientConfig,StreamOwned,RootCertStore,ClientConnection,ServerConfig,ServerConnection};
use rustls_pki_types::{ServerName,PrivateKeyDer,CertificateDer};
use rustls_pki_types::pem::PemObject;
use base64::prelude::*;

trait ReadWrite: Read + Write + Any {}
impl ReadWrite for TcpStream {}
impl ReadWrite for StreamOwned<ClientConnection,TcpStream> {}
impl ReadWrite for StreamOwned<ServerConnection,TcpStream> {}

pub struct SMTPServerConfig {
	auth_required: bool,
	check_user: Box<dyn Fn(&str) -> bool>, //takes username
	check_password: Box<dyn Fn(&str,&str) -> bool>, //takes username,password
	tls_enabled: bool,
	tls_certs: Option<String>, //both file paths
	tls_private_key: Option<String>,
	timeout: Duration,
}
impl SMTPServerConfig {
	pub fn set_auth_required(&mut self, state: bool){
		self.auth_required = state;
	}
	pub fn set_check_user_func(&mut self, func: impl Fn(&str) -> bool + 'static){
		self.check_user = Box::new(func);
	}
	pub fn set_check_password_func(&mut self, func: impl Fn(&str,&str) -> bool + 'static){
		self.check_password = Box::new(func);
	}
	pub fn configure_tls(&mut self, certs_file_path: &str, private_key_file_path: &str){
		self.tls_certs = Some(certs_file_path.to_string());
		self.tls_private_key = Some(private_key_file_path.to_string());
		self.tls_enabled = true;
	}
	pub fn set_timeout(&mut self, timeout: Duration){
		self.timeout = timeout;
	}
	pub fn timeout(&self) -> Duration {self.timeout.clone()}
}
impl Default for SMTPServerConfig {
	fn default() -> Self {
		SMTPServerConfig {
			auth_required: false,
			check_user: Box::new(|_| true),
			check_password: Box::new(|_,_| true),
			tls_enabled: false,
			tls_certs: None,
			tls_private_key: None,
			timeout: Duration::new(5,0), //5s
		}
	}
}

pub fn recieve_emails(mut connection: TcpStream, config: &SMTPServerConfig) -> Result<Vec<Email>,Box<dyn Error>>{
	//====== anounce existence ======
	connection.write(b"220 smtpserver at your service\r\n")?;
	//====== process mail ======
	//box connection (can be upgraded from TcpStream to tls Stream)
	let mut connection = Box::new(connection) as Box<dyn ReadWrite>;
	let mut emails = vec![];
	//multiple messages, one connection
	loop {
		let email = match smtp_receive_email(&mut connection,config){
			//error
			Err(e) => {
				if let Some(io_error) = e.downcast_ref::<io::Error>(){
					//if there is an unexpected EOF the client is done
					//so simply return the emails as the connection
					//is already closed.
					if io_error.kind() == io::ErrorKind::UnexpectedEof {
						return Ok(emails)
					}
				}
				return Err(e)
			},
			//successful receipt of new email
			Ok(email) => email,
		};
		if let Some(email) = email {
			emails.push(email);
			//mail has been stored
			connection.write(b"250 Ok\r\n")?;
		}else {break} //email is None so QUIT command was given
	}
	let _ = connection.write(b"221 Ending transaction\r\n");
	//====== close connection ======
	drop(connection);
	Ok(emails)
}

fn send_multipart(stream: &mut dyn Write, items: &[&str], code: &str) -> io::Result<()>{
	let lines = Vec::from(items);
	for (i,line) in lines.iter().enumerate() {
		let prefix = if i+1 == lines.len() {
			format!("{code} ")
		}else {
			format!("{code}-")
		};
		stream.write(format!("{prefix}{line}\r\n").as_bytes())?;
	}
	Ok(())
}

fn smtp_receive_email(connection: &mut Box<dyn ReadWrite>, config: &SMTPServerConfig) -> Result<Option<Email>,Box<dyn Error>>{
	//=> based off RFC 5321 <=//
	let mut senders: Vec<String> = vec![];
	let mut recipients: Vec<String> = vec![];
	let mut body = String::new();
	let mut authenticated = false;
	loop {
		let line = dbg!{readline(connection)?};
		let line_uppercase = line.to_ascii_uppercase();
		let (command,arg) = line_uppercase
			.split_once(' ')
			.unwrap_or((&line,""));
		match command {
			"QUIT" => {
				//====== end of mail ======
				return Ok(None)
			},
			"HELO" => {
				//====== HELO ======
				connection.write(b"250 Ok\r\n")?;
			},
			"EHLO" => {
				//====== extended hello EHLO ======
				//multi line response (250 then '-' except last with 250 then ' ')
				//====== capabilities ======
				let mut capabilities = vec!["smtpserver"]; //our fqdn (im cheating here)
				//enable auth if required
				if config.auth_required {
					capabilities.push("AUTH LOGIN");
				}
				//enable tls if supported
				if config.tls_enabled {
					capabilities.push("STARTTLS")
				}
				send_multipart(connection,&capabilities,"250")?;
			},
			"MAIL" if arg.starts_with("FROM") => {
				//check authentication status
				if config.auth_required && !authenticated {
					connection.write(b"530 Authentication required\r\n")?;
					continue;
				}
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
			},
			"RCPT" if arg.starts_with("TO") => {
				//check authentication status
				if config.auth_required && !authenticated {
					connection.write(b"530 Authentication required\r\n")?;
					continue;
				}
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
			},
			"DATA" => {
				//check authentication status
				if config.auth_required && !authenticated {
					connection.write(b"530 Authentication required\r\n")?;
					continue;
				}
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
			},
			"AUTH" if arg == "LOGIN" => {
				//====== authentication ======
				//ask for username
				connection.write(b"334 VXNlcm5hbWU6\r\n")?;
				let Ok(Ok(username)) = BASE64_STANDARD.decode(readline(connection)?).map(String::from_utf8)
				else {
					connection.write(b"501 Could not base64 decode username\r\n")?;
					continue;
				};
				//ask for password
				connection.write(b"334 UGFzc3dvcmQ6\r\n")?;
				let Ok(Ok(password)) = BASE64_STANDARD.decode(readline(connection)?).map(String::from_utf8)
				else {
					connection.write(b"501 Could not base64 decode password\r\n")?;
					continue;
				};
				//verify credentials
				if (config.check_user)(&username) && (config.check_password)(&username,&password) {
					//success
					connection.write(b"235 Authentication successfull\r\n")?;
					println!("authentication successfull");
					authenticated = true;
				}else {
					//epic authentication fail
					connection.write(b"535 Bad username or password\r\n")?;
					eprintln!("authentication failed");
					continue;
				}
			},
			"STARTTLS" if config.tls_enabled => {
				//====== upgrade connection to tls ======
				match <dyn Any>::downcast_ref::<TcpStream>(connection) {
					Some(mut tcp_stream) => {
						//stream is a plain TcpStream
						tcp_stream.write(b"220 Ready to start TLS\r\n")?;
						//clone in case tls_upgrade fails so we still have a stream to use
						let Ok(tcp_stream_clone) = tcp_stream.try_clone()
						else {
							eprintln!("failed to clone TcpStream");
							tcp_stream.write(b"451 Server Error (could not clone tcp stream)\r\n")?;
							continue;
						};
						//upgrade
						let new_connection = server_tls_upgrade(tcp_stream_clone,config)?;
						//move connection back outside this scope
						let old_connection = mem::replace(connection,Box::new(new_connection));
						//close old connection
						mem::drop(old_connection);
						println!("upgrade successfull");
					},
					None => {
						//stream already a tls connection
						connection.write(b"503 TLS already active")?;
						continue;
					}
				}
			},
			_ => {
				//====== command error ======
				connection.write(b"500 Unknown command\r\n")?;
				continue;
			}
		}
	}
	//====== construct the new email ======
	//is it empty?
	if senders.len() == 0 || recipients.len() == 0 {
		Ok(None)
	}else {
		let email = Email::new(senders,recipients,body);
		Ok(Some(email))
	}
}

//return a list of capabilities
fn smtp_ehlo(stream: &mut dyn ReadWrite) -> Result<Vec<String>,Box<dyn Error>> {
	let mut line = readline(stream)?;
	if !line.starts_with("220"){
		return Err(io::Error::other("failed smtp handshake"))?;
	}
	let mut capabilities = vec![];
	//====== attempt EHLO ======
	stream.write(b"EHLO smtprelay\r\n")?;
	line = readline(stream)?;
	if line.starts_with("2"){
		//====== read capability list ======
		loop {
			let line = readline(stream)?;
			//check for error
			if !line.starts_with("250") {
				return Err(io::Error::other(format!("Error completing handshake: {line}")))?
			}
			if line.len() >= 4 {capabilities.push(line[4..].to_string().to_ascii_uppercase())}
			//check for end of capabilities
			if line.starts_with("250 "){ //as opposed to "250-"
				break;
			}
		}
	}else {
		//====== fallback to HELO ======
		stream.write(b"HELO smtprelay\r\n")?;
		line = readline(stream)?;
		if !line.starts_with("250"){
			return Err(io::Error::other("failed smtp handshake"))?;
		}
	}
	Ok(capabilities)
}

pub fn send_emails(address: &str, emails: Vec<Email>) -> Result<(),Box<dyn Error>> {
	//====== connect ======
	let mut initial_connection = TcpStream::connect((address,25))?;
	//====== handshake ======
	let capabilities = smtp_ehlo(&mut initial_connection)?;
	//====== attempt to upgrade connection if possible ======
	let mut boxed_stream: Box<dyn ReadWrite> = Box::new(initial_connection.try_clone()?);
	//check for STARTTLS capability
	if capabilities.contains(&String::from("STARTTLS")){
		println!("==> upgrading connection to tls");
		initial_connection.write(b"STARTTLS\r\n")?;
		let line = readline(&mut initial_connection)?;
		if !line.starts_with("2"){
			return Err(io::Error::other(format!("failed STARTTLS: {}",line)))?;
		}
		boxed_stream = Box::new(client_tls_upgrade(address,initial_connection)?);
		println!("==> upgrade successfull");
	}
	//makes my life easier
	let stream = &mut *boxed_stream;
	//====== send emails ======
	let mut line;
	for email in emails {
		//====== senders ======
		for sender in email.senders_vec() {
			let mail_from = format!("MAIL FROM:<{}>\r\n",sender);
			stream.write(&mail_from.into_bytes())?;
			line = readline(stream)?;
			if !line.starts_with("250"){
				return Err(io::Error::other(String::from("server error: ")+&line))?;
			}
		}
		//====== recipients ======
		for recipient in email.recipients_vec() {
			let rcpt_to = format!("RCPT TO:<{}>\r\n",recipient);
			stream.write(&rcpt_to.into_bytes())?;
			line = readline(stream)?;
			if !line.starts_with("250"){
				return Err(io::Error::other(String::from("server error: ")+&line))?;
			}
		}
		//====== data ======
		stream.write(b"DATA\r\n")?;
		line = readline(stream)?;
		//wait for go ahead
		if !line.starts_with("354"){
			return Err(io::Error::other(String::from("server error: ")+&line))?;
		}
		stream.write(&email.data().into_bytes())?;
		stream.write(b"\r\n.\r\n")?;
		line = readline(stream)?;
		if !line.starts_with("250"){
			return Err(io::Error::other(String::from("server error: ")+&line))?;
		}
	}
	//====== quit ======
	stream.write(b"QUIT\r\n")?;
	//line = readline(stream)?;
	//if !line.starts_with("2"){
	//	return Err(io::Error::other(String::from("server error: ")+&line))?;
	//}
	Ok(())
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

fn client_tls_upgrade(destination: &str, connection: TcpStream) -> Result<StreamOwned<ClientConnection,TcpStream>,Box<dyn Error>> {
	let root_store = RootCertStore {
		roots: webpki_roots::TLS_SERVER_ROOTS.into(),
	};
	let config = ClientConfig::builder()
		.with_root_certificates(root_store)
		.with_no_client_auth();
	let name = ServerName::try_from(destination.to_string())?;
	let tls = ClientConnection::new(config.into(),name)?;
	Ok(StreamOwned::new(tls,connection))
}

fn server_tls_upgrade(connection: TcpStream, config: &SMTPServerConfig) -> Result<StreamOwned<ServerConnection,TcpStream>,Box<dyn Error>> {
	println!("starting tls upgrade...");
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
