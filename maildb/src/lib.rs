use std::time::{SystemTime,UNIX_EPOCH,Duration};
use rusqlite::{Connection,Error as sqlError,params};

pub struct Email {
	pub senders: Vec<String>,
	pub recipients: Vec<String>,
	pub body: String,
	pub timestamp: u64,
}

impl Default for Email {
	fn default() -> Self {
		let timestamp = SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.unwrap_or(Duration::default())
			.as_secs();
		Email {
			senders: vec![],
			recipients: vec![],
			body: String::new(),
			timestamp,
		}
	}
}

impl Email {
	pub fn timestamp(&self) -> u64 {self.timestamp}
	pub fn body(&self) -> String {self.body.clone()}
	pub fn senders_string(&self) -> String {
		self.senders
			.clone()
			.into_iter()
			.map(|sender| sender + ";") //semi colon seperated senders
			.fold(String::new(),|senders,sender| senders + &sender)
	}
	pub fn recipients_string(&self) -> String {
		self.recipients
			.clone()
			.into_iter()
			.map(|recipient| recipient + ";") //semi colon seperated recipients
			.fold(String::new(),|recipients,recipient| recipients + &recipient)
	}
}

pub struct MailDB {
	db: Connection,
}

impl MailDB {
	pub fn open(db_location: &str) -> Result<Self,sqlError> {
		let db_connection = Connection::open(db_location)?;
		//====== create relevant tables if they dont exist ======
		//emails table
		db_connection.execute("
		CREATE TABLE IF NOT EXISTS emails (
			id INTEGER PRIMARY KEY,
			receipt_timestamp INTEGER,
			senders TEXT,
			recipients TEXT,
			body TEXT
		)
		",[])?;
		//users table
		db_connection.execute("
		CREATE TABLE IF NOT EXISTS users (
			id INTEGER PRIMARY KEY,
			name TEXT
		)
		",[])?;
		//construct the db struct
		Ok(MailDB {
			db: db_connection,
		})
	}
	pub fn store_email(&self, email: Email) -> Result<(),sqlError> {
		self.db.execute("
		INSERT INTO emails (receipt_timestamp, senders, recipients, body)
		VALUES (?, ?, ?, ?)
		",params![
			email.timestamp() as i64,
			email.senders_string(),
			email.recipients_string(),
			email.body()
		])?;
		Ok(())
	}
}
